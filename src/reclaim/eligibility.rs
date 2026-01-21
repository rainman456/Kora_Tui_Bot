use solana_sdk::pubkey::Pubkey;
use chrono::{DateTime, Utc, Duration};
use crate::{
    error::Result,
    solana::{client::SolanaRpcClient, accounts::AccountDiscovery},
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
    /// 
    /// An account is eligible if:
    /// 1. It doesn't exist (closed), OR
    /// 2. It's empty (no meaningful data) AND inactive (no recent transactions)
    /// 3. It's not whitelisted or blacklisted
    pub async fn is_eligible(&self, pubkey: &Pubkey, created_at: DateTime<Utc>) -> Result<bool> {
        // Check whitelist first (never reclaim)
        if self.is_whitelisted(pubkey) {
            debug!("Account {} is whitelisted", pubkey);
            return Ok(false);
        }
        
        // Check blacklist (explicitly excluded)
        if self.is_blacklisted(pubkey) {
            debug!("Account {} is blacklisted", pubkey);
            return Ok(false);
        }
        
        // Check 1: Account is closed (doesn't exist)
        let account_exists = self.rpc_client.is_account_active(pubkey).await?;
        if !account_exists {
            debug!("Account {} is closed (doesn't exist)", pubkey);
            return Ok(true);
        }
        
        // Check 2a: Account is empty (check data and balance)
        if let Some(account) = self.rpc_client.get_account(pubkey).await? {
            let min_balance = self.rpc_client.get_minimum_balance_for_rent_exemption(account.data.len())?;
            let is_empty = crate::solana::rent::RentCalculator::is_empty_account(&account, min_balance);
            
            if is_empty {
                // Check 2b: Account is inactive
                if let Ok(inactive) = self.check_inactivity(pubkey).await {
                    if inactive {
                        debug!("Account {} is empty and inactive", pubkey);
                        return Ok(true);
                    }
                }
            }
        }
        
        // Check 3: Minimum inactive period (for all accounts)
        let now = Utc::now();
        let min_inactive = Duration::days(self.config.reclaim.min_inactive_days as i64);
        
        if now - created_at < min_inactive {
            debug!("Account {} hasn't been inactive long enough", pubkey);
            return Ok(false);
        }
        
        Ok(false)
    }
    
    /// Check if account has been inactive (no recent transactions)
    pub async fn check_inactivity(&self, pubkey: &Pubkey) -> Result<bool> {
        let discovery = AccountDiscovery::new(
            self.rpc_client.clone(),
            Pubkey::default(), // Not needed for this check
        );
        
        // Get last transaction time
        match discovery.get_last_transaction_time(pubkey).await? {
            Some(last_activity) => {
                let now = Utc::now();
                let min_inactive = Duration::days(self.config.reclaim.min_inactive_days as i64);
                let inactive = now - last_activity > min_inactive;
                
                debug!(
                    "Account {} last activity: {}, inactive: {}",
                    pubkey,
                    last_activity.format("%Y-%m-%d %H:%M:%S"),
                    inactive
                );
                
                Ok(inactive)
            }
            None => {
                // No transactions found - consider inactive
                debug!("Account {} has no transaction history", pubkey);
                Ok(true)
            }
        }
    }
    
    /// Check if account is whitelisted (protected from reclaim)
    fn is_whitelisted(&self, pubkey: &Pubkey) -> bool {
        self.config.reclaim.whitelist
            .iter()
            .any(|addr| addr == &pubkey.to_string())
    }
    
    /// Check if account is blacklisted (explicitly excluded)
    fn is_blacklisted(&self, pubkey: &Pubkey) -> bool {
        self.config.reclaim.blacklist
            .iter()
            .any(|addr| addr == &pubkey.to_string())
    }
    
    /// Get detailed eligibility reason
    pub async fn get_eligibility_reason(&self, pubkey: &Pubkey, created_at: DateTime<Utc>) -> Result<String> {
        if self.is_whitelisted(pubkey) {
            return Ok("Account is whitelisted (protected)".to_string());
        }
        
        if self.is_blacklisted(pubkey) {
            return Ok("Account is blacklisted (excluded)".to_string());
        }
        
        if !self.rpc_client.is_account_active(pubkey).await? {
            return Ok("Account is closed (eligible for reclaim)".to_string());
        }
        
        let now = Utc::now();
        let min_inactive = Duration::days(self.config.reclaim.min_inactive_days as i64);
        let age = now - created_at;
        
        if age < min_inactive {
            let days_remaining = (min_inactive - age).num_days();
            return Ok(format!("Account needs {} more days of inactivity", days_remaining));
        }
        
        // Check if empty
        if let Some(account) = self.rpc_client.get_account(pubkey).await? {
            let min_balance = self.rpc_client.get_minimum_balance_for_rent_exemption(account.data.len())?;
            let is_empty = crate::solana::rent::RentCalculator::is_empty_account(&account, min_balance);
            
            if !is_empty {
                return Ok("Account still has data or significant balance".to_string());
            }
        }
        
        // Check inactivity
        match self.check_inactivity(pubkey).await {
            Ok(true) => Ok("Eligible for reclaim (empty and inactive)".to_string()),
            Ok(false) => Ok("Account has recent activity".to_string()),
            Err(e) => Ok(format!("Could not determine activity: {}", e)),
        }
    }
}