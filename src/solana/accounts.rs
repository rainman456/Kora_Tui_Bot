use solana_sdk::{
    pubkey::Pubkey,
    signature::Signature,
};
use solana_transaction_status::{
    EncodedConfirmedTransactionWithStatusMeta,
    UiMessage,
};
use crate::{
    error::Result,
    solana::client::SolanaRpcClient,
};
use tracing::{info, debug};
use std::str::FromStr;
use chrono::{DateTime, Utc};

/// Discovers accounts created/sponsored by a specific fee payer
pub struct AccountDiscovery {
    rpc_client: SolanaRpcClient,
    fee_payer: Pubkey,
}

/// Information about a discovered sponsored account
#[derive(Debug, Clone)]
pub struct SponsoredAccountInfo {
    pub pubkey: Pubkey,
    pub creation_signature: Signature,
    pub creation_slot: u64,
    pub creation_time: DateTime<Utc>,
    pub initial_balance: u64,
    pub data_size: usize,
    pub account_type: AccountType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AccountType {
    System,
    SplToken,
    Other(Pubkey), // Store the program ID
}

impl AccountDiscovery {
    pub fn new(rpc_client: SolanaRpcClient, fee_payer: Pubkey) -> Self {
        Self { rpc_client, fee_payer }
    }
    
    /// Discover accounts sponsored by the fee payer from transaction history
    /// Scans transaction signatures and parses them to find account creations
    pub async fn discover_from_signatures(
        &self,
        max_signatures: usize,
    ) -> Result<Vec<SponsoredAccountInfo>> {
        info!("Discovering sponsored accounts for fee payer: {}", self.fee_payer);
        
        let mut all_sponsored = Vec::new();
        let mut before_signature: Option<Signature> = None;
        const BATCH_SIZE: usize = 1000; // Max per RPC call
        
        let mut total_fetched = 0;
        
        while total_fetched < max_signatures {
            let limit = std::cmp::min(BATCH_SIZE, max_signatures - total_fetched);
            
            // Fetch batch of signatures
            let signatures: Vec<solana_client::rpc_response::RpcConfirmedTransactionStatusWithSignature> = self.rpc_client.get_signatures_for_address(
                &self.fee_payer,
                before_signature,
                None,
                limit,
            ).await?;
            
            if signatures.is_empty() {
                break;
            }
            
            debug!("Processing batch of {} signatures", signatures.len());
            
            for sig_info in &signatures {
                if sig_info.err.is_some() {
                    continue;
                }
                
                let signature = Signature::from_str(&sig_info.signature)?;
                
                // Get full transaction details
                if let Some(tx) = self.rpc_client.get_transaction(&signature).await? {
                    let sponsored = self.parse_transaction_for_creations(&tx, signature).await?;
                    all_sponsored.extend(sponsored);
                }
            }
            
            total_fetched += signatures.len();
            
            // Set before_signature for next iteration (pagination)
            if let Some(last_sig) = signatures.last() {
                before_signature = Some(Signature::from_str(&last_sig.signature)?);
            }
            
            // If we got fewer than requested, we've reached the end
            if signatures.len() < limit {
                break;
            }
        }
        
        info!("Discovered {} sponsored accounts", all_sponsored.len());
        Ok(all_sponsored)
    }
    
    /// Parse a transaction to find account creation instructions
    async fn parse_transaction_for_creations(
        &self,
        tx: &EncodedConfirmedTransactionWithStatusMeta,
        signature: Signature,
    ) -> Result<Vec<SponsoredAccountInfo>> {
        let mut creations = Vec::new();
        
        // Extract transaction metadata
        let slot = tx.slot;
        let block_time = tx.block_time.unwrap_or(0);
        let creation_time = DateTime::from_timestamp(block_time, 0)
            .unwrap_or_else(|| Utc::now());
        
        let transaction = match &tx.transaction.transaction {
            solana_transaction_status::EncodedTransaction::Json(ui_tx) => ui_tx,
            _ => return Ok(creations),
        };
        
        let message = &transaction.message;
        let account_keys = self.extract_account_keys(message)?;
        
        if let UiMessage::Parsed(parsed_msg) = message {
            for instruction in &parsed_msg.instructions {
                if let Some(creation) = self.parse_instruction_for_creation(
                    instruction,
                    &account_keys,
                    signature,
                    slot,
                    creation_time,
                ).await? {
                    creations.push(creation);
                }
            }
        }
        
        Ok(creations)
    }
    
    /// Extract account keys from transaction message
    fn extract_account_keys(&self, message: &UiMessage) -> Result<Vec<Pubkey>> {
        match message {
            UiMessage::Parsed(parsed) => {
                parsed.account_keys.iter()
                    .map(|key| Pubkey::from_str(&key.pubkey))
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(|e| crate::error::ReclaimError::ParsePubkey(e))
            }
            UiMessage::Raw(raw) => {
                raw.account_keys.iter()
                    .map(|key| Pubkey::from_str(key))
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(|e| crate::error::ReclaimError::ParsePubkey(e))
            }
        }
    }
    
    /// Parse an instruction to detect account creation
    async fn parse_instruction_for_creation(
        &self,
        instruction: &solana_transaction_status::UiInstruction,
        _account_keys: &[Pubkey],
        signature: Signature,
        slot: u64,
        creation_time: DateTime<Utc>,
    ) -> Result<Option<SponsoredAccountInfo>> {
        use solana_transaction_status::{UiInstruction, UiParsedInstruction};
        use serde_json::Value;
        
        match instruction {
            UiInstruction::Parsed(parsed_instr_enum) => {
                // UiParsedInstruction is an enum that can be Parsed or PartiallyDecoded
                match parsed_instr_enum {
                    UiParsedInstruction::Parsed(parsed_instr) => {
                        let program = &parsed_instr.program;
                        let parsed_value = &parsed_instr.parsed;
                        
                        // Check for System program CreateAccount or CreateAccountWithSeed
                        if program == "system" {
                            if let Some(parsed_info) = parsed_value.as_object() {
                                let type_option: Option<&str> = parsed_info.get("type").and_then(|v| v.as_str());
                                if let Some(info_type) = type_option {
                                    if info_type == "createAccount" || info_type == "createAccountWithSeed" {
                                        // Extract new account pubkey from "info"
                                        let info_option: Option<&serde_json::Map<String, Value>> = parsed_info.get("info").and_then(|v| v.as_object());
                                        if let Some(info) = info_option {
                                            let new_account_option: Option<&str> = info.get("newAccount").and_then(|v| v.as_str());
                                            if let Some(new_account_str) = new_account_option {
                                                let new_account = Pubkey::from_str(new_account_str)?;
                                                let lamports_val: Option<u64> = info.get("lamports").and_then(|v| v.as_u64());
                                                let lamports = lamports_val.unwrap_or(0);
                                                
                                                let space_val: Option<u64> = info.get("space").and_then(|v| v.as_u64());
                                                let space = space_val.unwrap_or(0) as usize;
                                                
                                                return Ok(Some(SponsoredAccountInfo {
                                                    pubkey: new_account,
                                                    creation_signature: signature,
                                                    creation_slot: slot,
                                                    creation_time,
                                                    initial_balance: lamports,
                                                    data_size: space,
                                                    account_type: AccountType::System,
                                                }));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        
                        // Check for SPL Token InitializeAccount
                        if program == "spl-token" {
                            if let Some(parsed_info) = parsed_value.as_object() {
                                let type_option: Option<&str> = parsed_info.get("type").and_then(|v| v.as_str());
                                if let Some(info_type) = type_option {
                                    if info_type == "initializeAccount" {
                                        let info_option: Option<&serde_json::Map<String, Value>> = parsed_info.get("info").and_then(|v| v.as_object());
                                        if let Some(info) = info_option {
                                            let account_option: Option<&str> = info.get("account").and_then(|v| v.as_str());
                                            if let Some(account_str) = account_option {
                                                let account = Pubkey::from_str(account_str)?;
                                                
                                                // SPL Token accounts are typically 165 bytes
                                                return Ok(Some(SponsoredAccountInfo {
                                                    pubkey: account,
                                                    creation_signature: signature,
                                                    creation_slot: slot,
                                                    creation_time,
                                                    initial_balance: 0, // Will be set later
                                                    data_size: 165,
                                                    account_type: AccountType::SplToken,
                                                }));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        
                        // ✅ Handle other programs (inside the Parsed match arm)
                        if program != "system" && program != "spl-token" {
                            debug!("Found instruction from unknown program: {}", program);
                            
                            // Try to extract account creation from generic instruction
                            if let Some(parsed_info) = parsed_value.as_object() {
                                if let Some(info) = parsed_info.get("info").and_then(|v| v.as_object()) {
                                    if let Some(account_str) = info.get("account")
                                        .or_else(|| info.get("newAccount"))
                                        .and_then(|v| v.as_str()) 
                                    {
                                        let account = Pubkey::from_str(account_str)?;
                                        
                                        return Ok(Some(SponsoredAccountInfo {
                                            pubkey: account,
                                            creation_signature: signature,
                                            creation_slot: slot,
                                            creation_time,
                                            initial_balance: 0,
                                            data_size: 0,
                                            account_type: AccountType::Other(
                                                Pubkey::from_str(program).unwrap_or(solana_sdk::system_program::id())
                                            ),
                                        }));
                                    }
                                }
                            }
                        }
                    }
                    UiParsedInstruction::PartiallyDecoded(_) => {
                        // Skip partially decoded instructions
                    }
                }
            }
            UiInstruction::Compiled(_) => {
                // Skip compiled (non-parsed) instructions
            }
        }
        
        Ok(None)
    }
    
    /// Get the last transaction time for an account (for inactivity detection)
    pub async fn get_last_transaction_time(&self, address: &Pubkey) -> Result<Option<DateTime<Utc>>> {
        // Get the most recent signature for this address
        let signatures: Vec<solana_client::rpc_response::RpcConfirmedTransactionStatusWithSignature> = self.rpc_client.get_signatures_for_address(
            address,
            None,
            None,
            1, // Only need the most recent
        ).await?;
        
        if let Some(sig_info) = signatures.first() {
            if let Some(block_time) = sig_info.block_time {
                return Ok(DateTime::from_timestamp(block_time, 0));
            }
        }
        
        Ok(None)
    }
}