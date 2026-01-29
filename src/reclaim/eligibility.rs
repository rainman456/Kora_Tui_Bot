// src/reclaim/eligibility.rs - FIXED unused parameter

use solana_sdk::pubkey::Pubkey;
use chrono::{DateTime, Utc, Duration};
use crate::{
    error::Result,
    solana::{client::SolanaRpcClient, accounts::AccountDiscovery},
    config::Config,
    kora::types::AccountType,
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
        
        // Check if account type is reclaimable
        let account_type = self.determine_account_type(&account);
        if !self.is_reclaimable_type(&account_type) {
            debug!("Account {} is not reclaimable (type: {:?})", pubkey, account_type);
            return Ok(false);
        }
        
        // For SPL Token accounts, verify we have close authority
        if matches!(account_type, AccountType::SplToken) {
            if !self.has_close_authority(&account).await? {
                debug!("Account {} - operator doesn't have close authority", pubkey);
                return Ok(false);
            }
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
        if account.lamports <= min_balance * 2 {
            debug!("Account {} is eligible: has minimal balance and is inactive", pubkey);
            return Ok(true);
        }
        
        debug!("Account {} is not eligible: has significant data/balance", pubkey);
        Ok(false)
    }
    












    fn determine_account_type(&self, account: &solana_sdk::account::Account) -> AccountType {
        if account.owner == spl_token::id() && account.data.len() == 165 {
            AccountType::SplToken
        } else if account.owner == solana_sdk::system_program::id() {
            AccountType::System
        } else {
            AccountType::Other(account.owner)
        }
    }
    
    fn is_reclaimable_type(&self, account_type: &AccountType) -> bool {
        match account_type {
            AccountType::System => false,
            AccountType::SplToken => true,
            AccountType::Other(_) => false,
        }
    }


    // Add to impl EligibilityChecker in src/reclaim/eligibility.rs

/// Determine reclaim strategy for an account
pub async fn determine_reclaim_strategy(
    &self,
    pubkey: &Pubkey,
) -> Result<(crate::storage::models::ReclaimStrategy, Option<String>)> {
    let account = self.rpc_client.get_account(pubkey).await?;
    if account.is_none() {
        return Ok((crate::storage::models::ReclaimStrategy::Unknown, None));
    }
    
    let account = account.unwrap();
    let account_type = self.determine_account_type(&account);
    
    match account_type {
        AccountType::System => {
            // System accounts: user controls the keys
            Ok((
                crate::storage::models::ReclaimStrategy::Unrecoverable,
                None
            ))
        }
        
        AccountType::SplToken => {
            // Check if operator has close authority
            if self.has_close_authority(&account).await? {
                let operator = self.config.operator_pubkey()?;
                Ok((
                    crate::storage::models::ReclaimStrategy::ActiveReclaim,
                    Some(operator.to_string())
                ))
            } else {
                // Try to get the actual close authority
                let close_authority = self.get_token_close_authority(&account)?;
                Ok((
                    crate::storage::models::ReclaimStrategy::PassiveMonitoring,
                    close_authority
                ))
            }
        }
        
        AccountType::Other(_) => {
            // Custom programs: depends on program logic
            Ok((
                crate::storage::models::ReclaimStrategy::Unknown,
                None
            ))
        }
    }
}

/// Get the close authority from a token account
fn get_token_close_authority(&self, account: &solana_sdk::account::Account) -> Result<Option<String>> {
    if account.data.len() < 165 {
        return Ok(None);
    }
    
    let has_close_authority = account.data[129] == 1;
    
    if has_close_authority {
        let close_authority_bytes: [u8; 32] = account.data[130..162]
            .try_into()
            .map_err(|_| crate::error::ReclaimError::NotEligible(
                "Failed to parse close authority".to_string()
            ))?;
        let close_authority = Pubkey::new_from_array(close_authority_bytes);
        Ok(Some(close_authority.to_string()))
    } else {
        // No close authority set - owner is the authority
        let owner_bytes: [u8; 32] = account.data[32..64]
            .try_into()
            .map_err(|_| crate::error::ReclaimError::NotEligible(
                "Failed to parse owner".to_string()
            ))?;
        let owner = Pubkey::new_from_array(owner_bytes);
        Ok(Some(owner.to_string()))
    }
}



    
    /// ✅ FIX: Removed unused pubkey parameter
    async fn has_close_authority(
        &self,
        account: &solana_sdk::account::Account,
    ) -> Result<bool> {
        // SPL Token account layout:
        // CloseAuthority is at offset 129 (4 bytes for option + 32 bytes for pubkey)
        if account.data.len() < 165 {
            return Ok(false);
        }
        
        let has_close_authority = account.data[129] == 1;
        
        if has_close_authority {
            let close_authority_bytes: [u8; 32] = account.data[130..162]
                .try_into()
                .map_err(|_| crate::error::ReclaimError::NotEligible(
                    "Failed to parse close authority".to_string()
                ))?;
            let close_authority = Pubkey::new_from_array(close_authority_bytes);
            
            // Load operator pubkey from config
            let operator = self.config.operator_pubkey()?;
            
            Ok(close_authority == operator)
        } else {
            // No close authority set - check if operator is owner
            let owner_bytes: [u8; 32] = account.data[32..64]
                .try_into()
                .map_err(|_| crate::error::ReclaimError::NotEligible(
                    "Failed to parse owner".to_string()
                ))?;
            let owner = Pubkey::new_from_array(owner_bytes);
            
            let operator = self.config.operator_pubkey()?;
            Ok(owner == operator)
        }
    }
    
    pub async fn check_inactivity(&self, pubkey: &Pubkey) -> Result<bool> {
        let discovery = AccountDiscovery::new(
            self.rpc_client.clone(),
            Pubkey::default(),
        );
        
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
                debug!("Account {} has no transaction history", pubkey);
                Ok(true)
            }
        }
    }
    
    fn is_whitelisted(&self, pubkey: &Pubkey) -> bool {
        self.config.reclaim.whitelist
            .iter()
            .any(|addr| addr == &pubkey.to_string())
    }
    
    fn is_blacklisted(&self, pubkey: &Pubkey) -> bool {
        self.config.reclaim.blacklist
            .iter()
            .any(|addr| addr == &pubkey.to_string())
    }
    
    pub async fn get_eligibility_reason(&self, pubkey: &Pubkey, created_at: DateTime<Utc>) -> Result<String> {
        if self.is_whitelisted(pubkey) {
            return Ok("Account is whitelisted (protected)".to_string());
        }
        
        if self.is_blacklisted(pubkey) {
            return Ok("Account is blacklisted (excluded)".to_string());
        }
        
        let account = self.rpc_client.get_account(pubkey).await?;
        if account.is_none() {
            return Ok("Account is closed (nothing to reclaim)".to_string());
        }
        
        let account = account.unwrap();
        
        if account.lamports == 0 {
            return Ok("Account has zero balance (nothing to reclaim)".to_string());
        }
        
        // Check account type
        let account_type = self.determine_account_type(&account);
        if !self.is_reclaimable_type(&account_type) {
            return Ok(format!(
                "Account type {:?} cannot be reclaimed (operator doesn't control it)",
                account_type
            ));
        }
        
        // For SPL Token, check close authority - ✅ FIX: Pass only account
        if matches!(account_type, AccountType::SplToken) {
            if !self.has_close_authority(&account).await? {
                return Ok("Operator is not the close authority for this SPL Token account".to_string());
            }
        }
        
        let now = Utc::now();
        let min_inactive = Duration::days(self.config.reclaim.min_inactive_days as i64);
        let age = now - created_at;
        
        if age < min_inactive {
            let days_remaining = (min_inactive - age).num_days();
            return Ok(format!("Account needs {} more days of inactivity", days_remaining));
        }
        
        let is_inactive = self.check_inactivity(pubkey).await.unwrap_or(false);
        if !is_inactive {
            return Ok("Account has recent activity".to_string());
        }
        
        let min_balance = self.rpc_client.get_minimum_balance_for_rent_exemption(account.data.len())?;
        let is_empty = crate::solana::rent::RentCalculator::is_empty_account(&account, min_balance);
        
        if is_empty {
            return Ok(format!(
                "Eligible for reclaim: empty account with {} lamports",
                account.lamports
            ));
        }
        
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