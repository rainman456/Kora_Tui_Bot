use solana_sdk::pubkey::Pubkey;
use chrono::{DateTime, Utc, Duration};
use crate::{
    error::{Result, ReclaimError},
    solana::client::SolanaRpcClient,
    config::Config,
};
use tracing::debug;

pub struct EligibilityChecker {
    rpc_client: SolanaRpcClient,
    config: Config,
}

impl EligibilityChecker {
    pub fn new(rpc_client: SolanaRpcClient, config: Config) -> Self {
        Self { rpc_client, config }
    }
    
    /// Check if account is eligible for rent reclaim
    pub async fn is_eligible(&self, pubkey: &Pubkey, created_at: DateTime<Utc>) -> Result<bool> {
        // Check 1: Account must be closed
        if self.rpc_client.is_account_active(pubkey)? {
            debug!("Account {} is still active", pubkey);
            return Ok(false);
        }
        
        // Check 2: Must meet minimum inactive period
        let now = Utc::now();
        let min_inactive = Duration::days(self.config.reclaim.min_inactive_days as i64);
        
        if now - created_at < min_inactive {
            debug!("Account {} hasn't been inactive long enough", pubkey);
            return Ok(false);
        }
        
        // Check 3: Not on whitelist
        if self.is_whitelisted(pubkey) {
            debug!("Account {} is whitelisted", pubkey);
            return Ok(false);
        }
        
        Ok(true)
    }
    
    /// Check if account is whitelisted (protected from reclaim)
    fn is_whitelisted(&self, pubkey: &Pubkey) -> bool {
        self.config.reclaim.whitelist
            .iter()
            .any(|addr| addr == &pubkey.to_string())
    }
    
    /// Get detailed eligibility reason
    pub async fn get_eligibility_reason(&self, pubkey: &Pubkey, created_at: DateTime<Utc>) -> Result<String> {
        if self.rpc_client.is_account_active(pubkey)? {
            return Ok("Account is still active".to_string());
        }
        
        let now = Utc::now();
        let min_inactive = Duration::days(self.config.reclaim.min_inactive_days as i64);
        
        if now - created_at < min_inactive {
            let days_remaining = (min_inactive - (now - created_at)).num_days();
            return Ok(format!("Account needs {} more days of inactivity", days_remaining));
        }
        
        if self.is_whitelisted(pubkey) {
            return Ok("Account is whitelisted".to_string());
        }
        
        Ok("Eligible for reclaim".to_string())
    }
}