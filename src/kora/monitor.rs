use solana_sdk::pubkey::Pubkey;
use std::str::FromStr; // ✅ ADD THIS - needed for from_str() method
use crate::{
    error::Result,
    solana::{client::SolanaRpcClient, accounts::AccountDiscovery},
    kora::types::SponsoredAccountInfo,
};
use tracing::{info, debug, warn}; // ✅ ADD warn to imports

pub struct KoraMonitor {
    rpc_client: SolanaRpcClient,
    operator_pubkey: Pubkey,
}

impl KoraMonitor {
    pub fn new(rpc_client: SolanaRpcClient, operator_pubkey: Pubkey) -> Self {
        Self {
            rpc_client,
            operator_pubkey,
        }
    }
    
    /// Get all sponsored accounts by scanning transaction history
    pub async fn get_sponsored_accounts(&self, max_transactions: usize) -> Result<Vec<SponsoredAccountInfo>> {
        info!("Scanning for Kora-sponsored accounts...");
        
        let discovery = AccountDiscovery::new(
            self.rpc_client.clone(),
            self.operator_pubkey,
        );
        
        let discovered = discovery.discover_from_signatures(max_transactions).await?;
        
        let mut sponsored_accounts = Vec::new();
        for account_info in discovered {
            let last_activity = discovery.get_last_transaction_time(&account_info.pubkey).await?;
            
            sponsored_accounts.push(SponsoredAccountInfo {
                pubkey: account_info.pubkey,
                created_at: account_info.creation_time,
                rent_lamports: account_info.initial_balance,
                data_size: account_info.data_size,
                account_type: account_info.account_type.into(),
                last_activity,
                creation_signature: account_info.creation_signature,
                creation_slot: account_info.creation_slot,
            });
        }
        
        debug!("Found {} sponsored accounts", sponsored_accounts.len());
        Ok(sponsored_accounts)
    }
    
    pub async fn is_kora_sponsored(&self, pubkey: &Pubkey) -> Result<bool> {
        debug!("Checking if account {} was sponsored by Kora", pubkey);
        
        use solana_sdk::signature::Signature;
        
        // Strategy: Fetch signatures in reverse (oldest first) until we find creation tx
        // We'll paginate backwards to find the very first transaction
        
        let mut oldest_signature: Option<Signature> = None;
        let mut before: Option<Signature> = None;
        const BATCH_SIZE: usize = 1000;
        const MAX_ITERATIONS: usize = 10; // Prevent infinite loops (10k sigs max)
        
        // First, get initial batch to find oldest
        for iteration in 0..MAX_ITERATIONS {
            let signatures = self.rpc_client.get_signatures_for_address(
                pubkey,
                before,
                None,
                BATCH_SIZE,
            ).await?;
            
            if signatures.is_empty() {
                if iteration == 0 {
                    debug!("Account {} has no transaction history (might be unused)", pubkey);
                    return Ok(false);
                }
                break; // Reached the end
            }
            
            // Track the oldest signature we've seen
            if let Some(last_sig_info) = signatures.last() {
                oldest_signature = Some(Signature::from_str(&last_sig_info.signature)?);
                before = oldest_signature;
            }
            
            // If we got fewer than requested, we've reached the end
            if signatures.len() < BATCH_SIZE {
                break;
            }
        }
        
        // Now check the oldest (creation) transaction
        if let Some(creation_sig) = oldest_signature {
            match self.rpc_client.get_transaction(&creation_sig).await? {
                Some(tx) => {
                    // Check if transaction succeeded
                    if tx.transaction.meta.as_ref().map(|m| m.err.is_some()).unwrap_or(false) {
                        debug!(
                            "Creation transaction {} failed for account {} - account likely doesn't exist",
                            creation_sig, pubkey
                        );
                        return Ok(false);
                    }
                    
                    let transaction = match &tx.transaction.transaction {
                        solana_transaction_status::EncodedTransaction::Json(ui_tx) => ui_tx,
                        _ => {
                            debug!("Transaction not in JSON format, cannot verify sponsorship");
                            return Ok(false);
                        }
                    };
                    
                    // Extract fee payer (first account key)
                    let fee_payer = match &transaction.message {
                        solana_transaction_status::UiMessage::Parsed(parsed) => {
                            if let Some(first_key) = parsed.account_keys.first() {
                                Pubkey::from_str(&first_key.pubkey).ok()
                            } else {
                                None
                            }
                        }
                        solana_transaction_status::UiMessage::Raw(raw) => {
                            if let Some(first_key) = raw.account_keys.first() {
                                Pubkey::from_str(first_key).ok()
                            } else {
                                None
                            }
                        }
                    };
                    
                    if let Some(payer) = fee_payer {
                        let is_sponsored = payer == self.operator_pubkey;
                        debug!(
                            "Account {} creation tx {}: fee payer={}, operator={}, sponsored={}",
                            pubkey, creation_sig, payer, self.operator_pubkey, is_sponsored
                        );
                        return Ok(is_sponsored);
                    } else {
                        debug!("Could not extract fee payer from creation transaction");
                        return Ok(false);
                    }
                }
                None => {
                    debug!("Creation transaction {} not found for account {}", creation_sig, pubkey);
                    return Ok(false);
                }
            }
        }
        
        debug!("Could not determine creation transaction for account {}", pubkey);
        Ok(false)
    }
    
    /// Scan for new accounts since a checkpoint signature (incremental scanning)
    pub async fn scan_new_accounts(
        &self,
        since_signature: Option<solana_sdk::signature::Signature>,
        max_transactions: usize,
    ) -> Result<Vec<SponsoredAccountInfo>> {
        info!("Scanning for new sponsored accounts...");
        
        let discovery = AccountDiscovery::new(
            self.rpc_client.clone(),
            self.operator_pubkey,
        );
        
        let discovered = if let Some(since_sig) = since_signature {
            info!("Incremental scan since: {}", since_sig);
            discovery.discover_incremental(since_sig, max_transactions).await?
        } else {
            info!("Full scan (no checkpoint)");
            discovery.discover_from_signatures(max_transactions).await?
        };
        
        let mut sponsored_accounts = Vec::new();
        for account_info in discovered {
            let last_activity = discovery.get_last_transaction_time(&account_info.pubkey).await?;
            
            sponsored_accounts.push(SponsoredAccountInfo {
                pubkey: account_info.pubkey,
                created_at: account_info.creation_time,
                rent_lamports: account_info.initial_balance,
                data_size: account_info.data_size,
                account_type: account_info.account_type.into(),
                last_activity,
                creation_signature: account_info.creation_signature,
                creation_slot: account_info.creation_slot,
            });
        }
        
        debug!("Found {} sponsored accounts", sponsored_accounts.len());
        Ok(sponsored_accounts)
    }
    
    /// Get total rent locked across all accounts (optimized with batching)
    pub async fn get_total_locked_rent(&self, accounts: &[SponsoredAccountInfo]) -> Result<u64> {
        if accounts.is_empty() {
            return Ok(0);
        }
        
        // Batch fetch all accounts at once (up to RPC limits)
        const MAX_BATCH_SIZE: usize = 100; // Solana RPC limit
        let mut total = 0u64;
        
        let pubkeys: Vec<Pubkey> = accounts.iter().map(|a| a.pubkey).collect();
        
        // Process in batches
        for chunk in pubkeys.chunks(MAX_BATCH_SIZE) {
            debug!("Fetching batch of {} accounts", chunk.len());
            
            match self.rpc_client.get_multiple_accounts(chunk).await {
                Ok(account_data) => {
                    for account_opt in account_data {
                        if let Some(account) = account_opt {
                            total = total.saturating_add(account.lamports);
                        }
                    }
                }
                Err(e) => {
                    // Fallback to individual calls if batch fails
                    warn!("Batch fetch failed ({}), falling back to individual calls", e);
                    
                    for pubkey in chunk {
                        if let Ok(Some(account)) = self.rpc_client.get_account(pubkey).await {
                            total = total.saturating_add(account.lamports);
                        }
                    }
                }
            }
        }
        
        info!(
            "Total locked rent: {} lamports ({:.9} SOL)", 
            total, 
            crate::solana::rent::RentCalculator::lamports_to_sol(total)
        );
        
        Ok(total)
    }
}