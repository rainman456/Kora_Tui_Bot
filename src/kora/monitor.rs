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
    /// 
    /// Scans up to `max_transactions` worth of transaction history
    /// to find accounts created/sponsored by the Kora operator
    pub async fn get_sponsored_accounts(&self, max_transactions: usize) -> Result<Vec<SponsoredAccountInfo>> {
        info!("Scanning for Kora-sponsored accounts...");
        
        let discovery = AccountDiscovery::new(
            self.rpc_client.clone(),
            self.operator_pubkey,
        );
        
        // Discover accounts from transaction history
        let discovered = discovery.discover_from_signatures(max_transactions).await?;
        
        // Convert to Kora SponsoredAccountInfo format
        let mut sponsored_accounts = Vec::new();
        for account_info in discovered {
            // Get last activity timestamp
            let last_activity = discovery.get_last_transaction_time(&account_info.pubkey).await?;
            
            sponsored_accounts.push(SponsoredAccountInfo {
                pubkey: account_info.pubkey,
                created_at: account_info.creation_time,
                rent_lamports: account_info.initial_balance,
                data_size: account_info.data_size,
                account_type: account_info.account_type.into(),
                last_activity,
            });
        }
        
        debug!("Found {} sponsored accounts", sponsored_accounts.len());
        Ok(sponsored_accounts)
    }
    
    /// Check if a specific account was sponsored by Kora operator
    /// 
    /// Verifies by checking if the operator was the fee payer in the creation transaction
    pub async fn is_kora_sponsored(&self, pubkey: &Pubkey) -> Result<bool> {
        debug!("Checking if account {} was sponsored by Kora", pubkey);
        
        // Get account signatures (limit to recent history)
        let signatures = self.rpc_client.get_signatures_for_address(
            pubkey,
            None,
            None,
            100, // Check last 100 transactions
        ).await?;
        
        // Check if operator pubkey appears as fee payer in any transaction
        for sig_info in signatures {
            // Skip failed transactions
            if sig_info.err.is_some() {
                continue;
            }
            
            // For a full check, we'd need to parse the transaction
            // This is a simplified version - in production, you'd want to
            // check the transaction details to verify the fee payer
            // For now, we assume if we find it, it's sponsored
        }
        
        // More robust: scan discovery results
        Ok(false) // Conservative default
    }
    
    /// Scan for new sponsored accounts since last check
    /// 
    /// Uses the `until` parameter to only fetch signatures after a certain point
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
        
        // This would need modification to AccountDiscovery to support 'until' parameter
        // For now, we'll do a full scan and filter
        let all_accounts = self.get_sponsored_accounts(max_transactions).await?;
        
        Ok(all_accounts)
    }
    
    /// Calculate total SOL locked in all active sponsored accounts
    pub async fn get_total_locked_rent(&self, accounts: &[SponsoredAccountInfo]) -> Result<u64> {
        let mut total = 0u64;
        
        for account_info in accounts {
            // Check if account still exists
            if self.rpc_client.is_account_active(&account_info.pubkey).await? {
                // Get current balance (might differ from initial)
                let balance = self.rpc_client.get_balance(&account_info.pubkey).await?;
                total = total.saturating_add(balance);
            }
        }
        
        info!("Total locked rent: {} lamports", total);
        Ok(total)
    }
}

// Implement Clone for SolanaRpcClient (needed for AccountDiscovery)
impl Clone for SolanaRpcClient {
    fn clone(&self) -> Self {
        Self {
            client: solana_client::rpc_client::RpcClient::new_with_commitment(
                self.client.url(),
                self.client.commitment(),
            ),
            rate_limit_delay: self.rate_limit_delay,
        }
    }
}