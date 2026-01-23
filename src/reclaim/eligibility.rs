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
    /// 1. It exists (has balance to reclaim)
    /// 2. It's not whitelisted or blacklisted
    /// 3. It has been inactive for the minimum period
    /// 4. It's empty (no meaningful data) or has only rent balance
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
        
        let account = self.rpc_client.get_account(pubkey).await?;
        if account.is_none() {
            debug!("Account {} doesn't exist, nothing to reclaim", pubkey);
            return Ok(false);
        }
        
        let account = account.unwrap();
        
        // Account must have balance to reclaim
        if account.lamports == 0 {
            debug!("Account {} has zero balance", pubkey);
            return Ok(false);
        }
        
        let now = Utc::now();
        let min_inactive = Duration::days(self.config.reclaim.min_inactive_days as i64);
        
        if now - created_at < min_inactive {
            debug!("Account {} hasn't been inactive long enough (created: {})", pubkey, created_at);
            return Ok(false);
        }
        
        // Check last activity time
        let is_inactive = self.check_inactivity(pubkey).await.unwrap_or(false);
        if !is_inactive {
            debug!("Account {} has recent activity", pubkey);
            return Ok(false);
        }
        
        let min_balance = self.rpc_client.get_minimum_balance_for_rent_exemption(account.data.len())?;
        let is_empty = crate::solana::rent::RentCalculator::is_empty_account(&account, min_balance);
        
        if is_empty {
            debug!("Account {} is eligible: empty and inactive", pubkey);
            return Ok(true);
        }
        
        // Account has data but might still be reclaimable if it's only rent
        // Allow reclaim if balance is close to minimum rent-exempt amount
        if account.lamports <= min_balance * 2 {
            debug!("Account {} is eligible: has minimal balance and is inactive", pubkey);
            return Ok(true);
        }
        
        debug!("Account {} is not eligible: has significant data/balance", pubkey);
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
        
        // Check if account exists
        let account = self.rpc_client.get_account(pubkey).await?;
        if account.is_none() {
            return Ok("Account is closed (nothing to reclaim)".to_string());
        }
        
        let account = account.unwrap();
        
        // Check balance
        if account.lamports == 0 {
            return Ok("Account has zero balance (nothing to reclaim)".to_string());
        }
        
        let now = Utc::now();
        let min_inactive = Duration::days(self.config.reclaim.min_inactive_days as i64);
        let age = now - created_at;
        
        if age < min_inactive {
            let days_remaining = (min_inactive - age).num_days();
            return Ok(format!("Account needs {} more days of inactivity", days_remaining));
        }
        
        // Check inactivity
        let is_inactive = self.check_inactivity(pubkey).await.unwrap_or(false);
        if !is_inactive {
            return Ok("Account has recent activity".to_string());
        }
        
        // Check if empty
        let min_balance = self.rpc_client.get_minimum_balance_for_rent_exemption(account.data.len())?;
        let is_empty = crate::solana::rent::RentCalculator::is_empty_account(&account, min_balance);
        
        if is_empty {
            return Ok(format!(
                "Eligible for reclaim: empty account with {} lamports",
                account.lamports
            ));
        }
        
        // Check if balance is minimal
        if account.lamports <= min_balance * 2 {
            return Ok(format!(
                "Eligible for reclaim: minimal balance ({} lamports)",
                account.lamports
            ));
        }
        
        Ok(format!(
            "Not eligible: account has significant data/balance ({} lamports, {} bytes data)",
            account.lamports,
            account.data.len()
        ))
    }
}