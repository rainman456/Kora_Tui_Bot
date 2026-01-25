use solana_sdk::pubkey::Pubkey;
use crate::{
    error::Result,
    solana::{client::SolanaRpcClient, accounts::AccountDiscovery},
    kora::types::SponsoredAccountInfo,
};
use tracing::{info, debug};

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
        
        use std::str::FromStr;
        use solana_sdk::signature::Signature;
        
        let signatures = self.rpc_client.get_signatures_for_address(
            pubkey,
            None,
            None,
            100,
        ).await?;
        
        if signatures.is_empty() {
            debug!("No signatures found for account {}", pubkey);
            return Ok(false);
        }
        
        let oldest_sig = match signatures.last() {
            Some(sig) => sig,
            None => return Ok(false),
        };

        if oldest_sig.err.is_some() {
            debug!("Creation transaction failed for account {}", pubkey);
            return Ok(false);
        }
        
        let signature = Signature::from_str(&oldest_sig.signature)?;
        if let Some(tx) = self.rpc_client.get_transaction(&signature).await? {
            let transaction = match &tx.transaction.transaction {
                solana_transaction_status::EncodedTransaction::Json(ui_tx) => ui_tx,
                _ => return Ok(false),
            };
            
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
                    "Account {} fee payer: {}, operator: {}, sponsored: {}",
                    pubkey, payer, self.operator_pubkey, is_sponsored
                );
                return Ok(is_sponsored);
            }
        }
        
        Ok(false)
    }
    
    pub async fn scan_new_accounts(
        &self,
        _since_signature: Option<solana_sdk::signature::Signature>,
        max_transactions: usize,
    ) -> Result<Vec<SponsoredAccountInfo>> {
        info!("Scanning for new sponsored accounts...");
        let all_accounts = self.get_sponsored_accounts(max_transactions).await?;
        Ok(all_accounts)
    }
    
    pub async fn get_total_locked_rent(&self, accounts: &[SponsoredAccountInfo]) -> Result<u64> {
        let mut total = 0u64;
        
        for account_info in accounts {
            if self.rpc_client.is_account_active(&account_info.pubkey).await? {
                let balance = self.rpc_client.get_balance(&account_info.pubkey).await?;
                total = total.saturating_add(balance);
            }
        }
        
        info!("Total locked rent: {} lamports", total);
        Ok(total)
    }
}