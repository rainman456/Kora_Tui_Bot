use solana_sdk::{
    pubkey::Pubkey,
    signature::Signature,
};
use solana_transaction_status::{
    EncodedConfirmedTransactionWithStatusMeta,
    UiTransactionStatusMeta,
    UiMessage,
    UiCompiledInstruction,
};
use crate::{
    error::Result,
    solana::client::SolanaRpcClient,
};
use tracing::{info, debug, warn};
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
            let signatures = self.rpc_client.get_signatures_for_address(
                &self.fee_payer,
                before_signature,
                None,
                limit,
            ).await?;
            
            if signatures.is_empty() {
                break;
            }
            
            debug!("Processing batch of {} signatures", signatures.len());
            
            // Parse each transaction to find account creations
            for sig_info in &signatures {
                // Skip failed transactions
                if sig_info.err.is_some() {
                    continue;
                }
                
                let signature = Signature::from_str(&sig_info.signature)
                    .map_err(|e| crate::error::ReclaimError::Other(anyhow::anyhow!("Invalid signature: {}", e)))?;
                
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
        
        // Parse transaction message
        let transaction = match &tx.transaction.transaction {
            solana_transaction_status::EncodedTransaction::Json(ui_tx) => ui_tx,
            _ => return Ok(creations), // Skip non-JSON encodings
        };
        
        let message = &transaction.message;
        let account_keys = self.extract_account_keys(message)?;
        
        // Parse instructions to find account creations
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
                    .map_err(|e| crate::error::ReclaimError::Other(anyhow::anyhow!("Invalid pubkey: {}", e)))
            }
            UiMessage::Raw(raw) => {
                raw.account_keys.iter()
                    .map(|key| Pubkey::from_str(key))
                    .collect::<std::result::Result<Vec<_>, _>>()
                    .map_err(|e| crate::error::ReclaimError::Other(anyhow::anyhow!("Invalid pubkey: {}", e)))
            }
        }
    }
    
    /// Parse an instruction to detect account creation
    async fn parse_instruction_for_creation(
        &self,
        instruction: &solana_transaction_status::UiInstruction,
        account_keys: &[Pubkey],
        signature: Signature,
        slot: u64,
        creation_time: DateTime<Utc>,
    ) -> Result<Option<SponsoredAccountInfo>> {
        use solana_transaction_status::UiInstruction;
        
        match instruction {
            UiInstruction::Parsed(parsed) => {
                // Check for System program CreateAccount or CreateAccountWithSeed
                if parsed.program == "system" {
                    if let Some(parsed_info) = parsed.parsed.as_object() {
                        if let Some(info_type) = parsed_info.get("type").and_then(|v| v.as_str()) {
                            if info_type == "createAccount" || info_type == "createAccountWithSeed" {
                                // Extract new account pubkey from "info"
                                if let Some(info) = parsed_info.get("info").and_then(|v| v.as_object()) {
                                    if let Some(new_account_str) = info.get("newAccount").and_then(|v| v.as_str()) {
                                        let new_account = Pubkey::from_str(new_account_str)?;
                                        let lamports = info.get("lamports").and_then(|v| v.as_u64()).unwrap_or(0);
                                        let space = info.get("space").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                                        
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
                if parsed.program == "spl-token" {
                    if let Some(parsed_info) = parsed.parsed.as_object() {
                        if let Some(info_type) = parsed_info.get("type").and_then(|v| v.as_str()) {
                            if info_type == "initializeAccount" {
                                if let Some(info) = parsed_info.get("info").and_then(|v| v.as_object()) {
                                    if let Some(account_str) = info.get("account").and_then(|v| v.as_str()) {
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
            }
            _ => {}
        }
        
        Ok(None)
    }
    
    /// Get the last transaction time for an account (for inactivity detection)
    pub async fn get_last_transaction_time(&self, address: &Pubkey) -> Result<Option<DateTime<Utc>>> {
        // Get the most recent signature for this address
        let signatures = self.rpc_client.get_signatures_for_address(
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
