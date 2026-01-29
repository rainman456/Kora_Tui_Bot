// src/solana/accounts.rs - Enhanced with RateLimiter

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
    utils::RateLimiter, 
};
use tracing::{info, debug, warn};
use std::str::FromStr;
use std::collections::HashSet;
use chrono::{DateTime, Utc};

// Constants for hardcoded values
const ATA_RENT_EXEMPTION: u64 = 2_039_280; // ~0.00203928 SOL
const ATA_SIZE: usize = 165;

/// Discovers accounts created/sponsored by a specific fee payer
pub struct AccountDiscovery {
    rpc_client: SolanaRpcClient,
    fee_payer: Pubkey,
    rate_limiter: RateLimiter, 
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
    Other(Pubkey),
}

impl AccountDiscovery {
    pub fn new(rpc_client: SolanaRpcClient, fee_payer: Pubkey) -> Self {
        // Use the RPC client's rate limit delay
        let rate_limit_ms = rpc_client.rate_limit_delay.as_millis() as u64;
        
        Self { 
            rpc_client, 
            fee_payer,
            rate_limiter: RateLimiter::new(rate_limit_ms), 
        }
    }
    
    /// Discover accounts sponsored by the fee payer from transaction history
    pub async fn discover_from_signatures(
        &self,
        max_signatures: usize,
    ) -> Result<Vec<SponsoredAccountInfo>> {
        info!("Discovering sponsored accounts for fee payer: {}", self.fee_payer);
        
        let mut all_sponsored = Vec::new();
        let mut seen_accounts = HashSet::new();  // Track seen accounts to prevent duplicates
        let mut before_signature: Option<Signature> = None;
        const BATCH_SIZE: usize = 1000;
        
        let mut total_fetched = 0;
        
        while total_fetched < max_signatures {
            let limit = std::cmp::min(BATCH_SIZE, max_signatures - total_fetched);
            
            
            self.rate_limiter.wait().await;
            
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
            
            for sig_info in &signatures {
                if sig_info.err.is_some() {
                    continue;
                }
                
                let signature = Signature::from_str(&sig_info.signature)?;
                
                // ✅ USE: wait() - Rate limit transaction fetches
                self.rate_limiter.wait().await;
                
                // Get full transaction details
                if let Some(tx) = self.rpc_client.get_transaction(&signature).await? {
                    let sponsored = self.parse_transaction_for_creations(&tx, signature).await?;
                    // Only add accounts we haven't seen before
                    for account_info in sponsored {
                        if seen_accounts.insert(account_info.pubkey) {
                            all_sponsored.push(account_info);
                        }
                    }
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
    
    /// Discover accounts created AFTER a specific signature (incremental scanning)
    pub async fn discover_incremental(
        &self,
        since_signature: Signature,
        max_signatures: usize,
    ) -> Result<Vec<SponsoredAccountInfo>> {
        info!("Discovering new sponsored accounts since signature: {}", since_signature);
        
        let mut all_sponsored = Vec::new();
        let mut seen_accounts = HashSet::new();  // Track seen accounts to prevent duplicates
        let mut before_signature: Option<Signature> = None;
        const BATCH_SIZE: usize = 1000;
        
        let mut total_fetched = 0;
        
        while total_fetched < max_signatures {
            let limit = std::cmp::min(BATCH_SIZE, max_signatures - total_fetched);
            
            // ✅ USE: wait() - Rate limit signature fetches
            self.rate_limiter.wait().await;
            
            // Fetch signatures UNTIL we reach since_signature
            let signatures = self.rpc_client.get_signatures_for_address(
                &self.fee_payer,
                before_signature,
                Some(since_signature),
                limit,
            ).await?;
            
            if signatures.is_empty() {
                debug!("No new signatures found since checkpoint");
                break;
            }
            
            debug!("Processing batch of {} new signatures", signatures.len());
            
            for sig_info in &signatures {
                if sig_info.err.is_some() {
                    continue;
                }
                
                let signature = Signature::from_str(&sig_info.signature)?;
                
                // ✅ USE: wait() - Rate limit transaction fetches
                self.rate_limiter.wait().await;
                
                // Get full transaction details
                if let Some(tx) = self.rpc_client.get_transaction(&signature).await? {
                    let sponsored = self.parse_transaction_for_creations(&tx, signature).await?;
                    // Only add accounts we haven't seen before
                    for account_info in sponsored {
                        if seen_accounts.insert(account_info.pubkey) {
                            all_sponsored.push(account_info);
                        }
                    }
                }
            }
            
            total_fetched += signatures.len();
            
            // Pagination
            if let Some(last_sig) = signatures.last() {
                before_signature = Some(Signature::from_str(&last_sig.signature)?);
            }
            
            // If we got fewer results than requested, we've reached the end
            if signatures.len() < limit {
                break;
            }
        }
        
        info!("Incremental scan discovered {} new sponsored accounts", all_sponsored.len());
        Ok(all_sponsored)
    }
    
    /// Parse a transaction to find account creation instructions
    async fn parse_transaction_for_creations(
        &self,
        tx: &EncodedConfirmedTransactionWithStatusMeta,
        signature: Signature,
    ) -> Result<Vec<SponsoredAccountInfo>> {
        let mut creations = Vec::new();
        
        let slot = tx.slot;
        let block_time = tx.block_time.unwrap_or(0);
        
        // CRITICAL: Do NOT use Utc::now() as fallback - it breaks inactivity calculations!
        // If block_time is missing, estimate from slot (each slot is ~400ms)
        let creation_time = if block_time > 0 {
            DateTime::from_timestamp(block_time, 0)
                .unwrap_or_else(|| {
                    warn!("Invalid block_time {} for slot {}, using slot-based estimation", block_time, slot);
                    // Estimate: slot 0 was around Sept 2020, each slot ~400ms
                    let estimated_seconds = (slot as i64 * 400) / 1000;
                    DateTime::from_timestamp(1600000000 + estimated_seconds, 0)
                        .unwrap_or_else(|| DateTime::from_timestamp(0, 0).unwrap())
                })
        } else {
            warn!("Missing block_time for slot {}, using slot-based estimation", slot);
            // Estimate from slot number
            let estimated_seconds = (slot as i64 * 400) / 1000;
            DateTime::from_timestamp(1600000000 + estimated_seconds, 0)
                .unwrap_or_else(|| DateTime::from_timestamp(0, 0).unwrap())
        };
        
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
            match parsed_instr_enum {
                UiParsedInstruction::Parsed(parsed_instr) => {
                    let program = &parsed_instr.program;
                    let parsed_value = &parsed_instr.parsed;
                    
                    // ✅ PRIORITY 1: Check for spl-associated-token-account (this is what Kora uses!)
                    if program == "spl-associated-token-account" {
                        if let Some(parsed_info) = parsed_value.as_object() {
                            let type_option: Option<&str> = parsed_info.get("type").and_then(|v| v.as_str());
                            if let Some(info_type) = type_option {
                                // Both "create" and "createIdempotent" create ATAs
                                if info_type == "create" || info_type == "createIdempotent" {
                                    let info_option: Option<&serde_json::Map<String, Value>> = 
                                        parsed_info.get("info").and_then(|v| v.as_object());
                                    if let Some(info) = info_option {
                                        // The ATA address is in the "account" field
                                        let account_option: Option<&str> = 
                                            info.get("account").and_then(|v| v.as_str());
                                        if let Some(account_str) = account_option {
                                            let ata_address = Pubkey::from_str(account_str)?;
                                            
                                            debug!("✓ Found ATA creation: {}", ata_address);
                                            
                                            // ATAs are 165 bytes and typically have ~0.00203928 SOL rent
                                            return Ok(Some(SponsoredAccountInfo {
                                                pubkey: ata_address,
                                                creation_signature: signature,
                                                creation_slot: slot,
                                                creation_time,
                                                initial_balance: ATA_RENT_EXEMPTION,
                                                data_size: ATA_SIZE,
                                                account_type: AccountType::SplToken,
                                            }));
                                        }
                                    }
                                }
                            }
                        }
                        
                        debug!("Found spl-associated-token-account instruction but couldn't parse account address");
                        return Ok(None);
                    }
                    
                    // Check for System program CreateAccount
                    if program == "system" {
                        if let Some(parsed_info) = parsed_value.as_object() {
                            let type_option: Option<&str> = parsed_info.get("type").and_then(|v| v.as_str());
                            if let Some(info_type) = type_option {
                                if info_type == "createAccount" || info_type == "createAccountWithSeed" {
                                    let info_option: Option<&serde_json::Map<String, Value>> = 
                                        parsed_info.get("info").and_then(|v| v.as_object());
                                    if let Some(info) = info_option {
                                        let new_account_option: Option<&str> = 
                                            info.get("newAccount").and_then(|v| v.as_str());
                                        if let Some(new_account_str) = new_account_option {
                                            let new_account = Pubkey::from_str(new_account_str)?;
                                            let lamports = info.get("lamports").and_then(|v| v.as_u64()).unwrap_or(0);
                                            let space = info.get("space").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
                                            
                                            debug!("✓ Found system account creation: {}", new_account);
                                            
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
                    
                    // Check for SPL Token InitializeAccount (less common, but still valid)
                    if program == "spl-token" {
                        if let Some(parsed_info) = parsed_value.as_object() {
                            let type_option: Option<&str> = parsed_info.get("type").and_then(|v| v.as_str());
                            if let Some(info_type) = type_option {
                                if info_type == "initializeAccount" {
                                    let info_option: Option<&serde_json::Map<String, Value>> = 
                                        parsed_info.get("info").and_then(|v| v.as_object());
                                    if let Some(info) = info_option {
                                        let account_option: Option<&str> = 
                                            info.get("account").and_then(|v| v.as_str());
                                        if let Some(account_str) = account_option {
                                            let account = Pubkey::from_str(account_str)?;
                                            
                                            debug!("✓ Found token account initialization: {}", account);
                                            
                                            return Ok(Some(SponsoredAccountInfo {
                                                pubkey: account,
                                                creation_signature: signature,
                                                creation_slot: slot,
                                                creation_time,
                                                initial_balance: 0, // We can't determine balance from initializeAccount alone
                                                data_size: ATA_SIZE,
                                                account_type: AccountType::SplToken,
                                            }));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    
                    // ✅ IMPROVED: More selective "Other" program detection
                    // Only capture if it's clearly an account CREATION instruction
                    if program != "system" 
                        && program != "spl-token" 
                        && program != "spl-associated-token-account" 
                    {
                        if let Some(parsed_info) = parsed_value.as_object() {
                            let type_option: Option<&str> = parsed_info.get("type").and_then(|v| v.as_str());
                            
                            // Only process if the instruction type indicates creation
                            if let Some(info_type) = type_option {
                                let is_creation = info_type.contains("create") 
                                    || info_type.contains("initialize")
                                    || info_type.contains("init");
                                
                                if is_creation {
                                    if let Some(info) = parsed_info.get("info").and_then(|v| v.as_object()) {
                                        // Look for common account creation patterns
                                        let account_key = info.get("account")
                                            .or_else(|| info.get("newAccount"))
                                            .or_else(|| info.get("address"))
                                            .and_then(|v| v.as_str());
                                        
                                        if let Some(account_str) = account_key {
                                            if let Ok(account_pubkey) = Pubkey::from_str(account_str) {
                                                // Try to parse the program ID
                                                if let Ok(program_id) = Pubkey::from_str(program) {
                                                    debug!("✓ Detected account creation from program: {} (type: {})", program, info_type);
                                                    
                                                    return Ok(Some(SponsoredAccountInfo {
                                                        pubkey: account_pubkey,
                                                        creation_signature: signature,
                                                        creation_slot: slot,
                                                        creation_time,
                                                        initial_balance: info.get("lamports").and_then(|v| v.as_u64()).unwrap_or(0),
                                                        data_size: info.get("space").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
                                                        account_type: AccountType::Other(program_id),
                                                    }));
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            
                            // Only log if we're actually looking at parsed info
                            if parsed_info.get("info").is_some() {
                                debug!("Found instruction from program: {} (no account creation detected)", program);
                            }
                        }
                    }
                }
                UiParsedInstruction::PartiallyDecoded(_) => {}
            }
        }
        UiInstruction::Compiled(_) => {}
    }
    
    Ok(None)
}
    
    /// Get the last transaction time for an account (for inactivity detection)
    pub async fn get_last_transaction_time(&self, address: &Pubkey) -> Result<Option<DateTime<Utc>>> {
        // ✅ USE: wait() - Rate limit before fetching signatures
        self.rate_limiter.wait().await;
        
        let signatures = self.rpc_client.get_signatures_for_address(
            address,
            None,
            None,
            1,
        ).await?;
        
        if let Some(sig_info) = signatures.first() {
            if let Some(block_time) = sig_info.block_time {
                return Ok(DateTime::from_timestamp(block_time, 0));
            }
        }
        
        Ok(None)
    }
}