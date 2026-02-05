// src/reclaim/eligibility.rs - FIXED with proper token balance checks in get_eligibility_reason

use solana_sdk::{pubkey::Pubkey, program_pack::Pack};
use chrono::{DateTime, Utc, Duration};
use spl_token::state::Account as TokenAccount;
use solana_sdk::program_option::COption;
use crate::{
    error::{Result, ReclaimError},
    solana::{client::SolanaRpcClient, accounts::AccountDiscovery},
    config::Config,
    kora::types::AccountType,
};
use tracing::{debug, warn};

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
       if self.is_blacklisted(pubkey) {
        debug!("Account {} is blacklisted", pubkey);
        return Ok(false);
    }
    
    // Whitelist check - if whitelist exists and is not empty, ONLY reclaim whitelisted accounts
    if !self.config.reclaim.whitelist.is_empty() {
        if !self.is_whitelisted(pubkey) {
            debug!("Account {} not on whitelist", pubkey);
            return Ok(false);
        }
    }
        
        let account = self.rpc_client.get_account(pubkey).await?;
if account.is_none() {
    return Err(ReclaimError::AccountNotFound(
        format!("Account {} does not exist", pubkey)
    ));
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
        
        // For SPL Token accounts, verify token balance and close authority
        if matches!(account_type, AccountType::SplToken) {
            // CRITICAL: Check if token account has zero token balance
            // SPL Token amount is stored at bytes 64-71 as u64 little-endian
            if account.data.len() >= 72 {
                let amount_bytes: [u8; 8] = account.data[64..72]
                    .try_into()
                    .map_err(|_| ReclaimError::NotEligible(
                        "Failed to parse token amount".to_string()
                    ))?;
                let token_amount = u64::from_le_bytes(amount_bytes);
                
                if token_amount > 0 {
                    debug!("Account {} still holds {} tokens, not eligible for reclaim", pubkey, token_amount);
                    return Ok(false);
                }
            }
            
            // Verify operator has close authority
            if !self.has_close_authority(&account)? {
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
        
        // Check last activity time with improved error handling
        let is_inactive = match self.check_inactivity(pubkey).await {
            Ok(inactive) => inactive,
            Err(e) => {
                tracing::warn!("Failed to check inactivity for {}: {}. Assuming active to be conservative.", pubkey, e);
                // Conservative: assume active on error to avoid premature reclaim
                false
            }
        };
        
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
        
        // Account has data but might still be reclaimable if balance is minimal
        // Allow reclaim if balance is <= 2x rent exemption (catches accounts with dust beyond rent)
        // This threshold ensures we don't reclaim accounts with significant user deposits
        if account.lamports <= min_balance * 2 {
            debug!("Account {} is eligible: has minimal balance ({} lamports, {} SOL) and is inactive", 
                   pubkey, account.lamports, account.lamports as f64 / 1_000_000_000.0);
            return Ok(true);
        }
        
        debug!("Account {} is not eligible: has significant data/balance", pubkey);
        Ok(false)
    }

    fn determine_account_type(&self, account: &solana_sdk::account::Account) -> AccountType {
        if account.owner == spl_token::id() && account.data.len() >= 165 {
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
                if self.has_close_authority(&account)? {
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
                .map_err(|_| ReclaimError::NotEligible(
                    "Failed to parse close authority".to_string()
                ))?;
            let close_authority = Pubkey::new_from_array(close_authority_bytes);
            Ok(Some(close_authority.to_string()))
        } else {
            // No close authority set - owner is the authority
            let owner_bytes: [u8; 32] = account.data[32..64]
                .try_into()
                .map_err(|_| ReclaimError::NotEligible(
                    "Failed to parse owner".to_string()
                ))?;
            let owner = Pubkey::new_from_array(owner_bytes);
            Ok(Some(owner.to_string()))
        }
    }

    fn has_close_authority(&self, account: &solana_sdk::account::Account) -> Result<bool> {
        let token_account = TokenAccount::unpack(&account.data)
            .map_err(|e| ReclaimError::NotEligible(format!("Failed to deserialize: {}", e)))?;
        
        let operator = self.config.operator_pubkey()?;
        
        // Now COption is recognized
        match token_account.close_authority {
            COption::None => Ok(token_account.owner == operator),
            COption::Some(close_auth) => Ok(close_auth == operator),
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
    
    /// FIXED: Get detailed eligibility reason with proper token balance checking
    pub async fn get_eligibility_reason(&self, pubkey: &Pubkey, created_at: DateTime<Utc>) -> Result<String> {
        // Check blacklist first
        if self.is_blacklisted(pubkey) {
            return Ok("Account is blacklisted (excluded from reclaim)".to_string());
        }
        
        // Check whitelist
        if !self.config.reclaim.whitelist.is_empty() && !self.is_whitelisted(pubkey) {
            return Ok("Account is not on whitelist (only whitelisted accounts can be reclaimed)".to_string());
        }
        
        let account = self.rpc_client.get_account(pubkey).await?;
        if account.is_none() {
            return Ok("Account is closed (nothing to reclaim)".to_string());
        }
        
        let account = account.unwrap();
        
        if account.lamports == 0 {
            return Ok("Account has zero lamports (nothing to reclaim)".to_string());
        }
        
        // Check account type
        let account_type = self.determine_account_type(&account);
        if !self.is_reclaimable_type(&account_type) {
            return Ok(format!(
                "Account type {:?} cannot be reclaimed (operator doesn't control it)",
                account_type
            ));
        }
        
        // CRITICAL FIX: Check token balance FIRST for SPL Token accounts
        if matches!(account_type, AccountType::SplToken) {
            // Check if token account has zero token balance
            if account.data.len() >= 72 {
                let amount_bytes: [u8; 8] = account.data[64..72]
                    .try_into()
                    .map_err(|_| ReclaimError::NotEligible(
                        "Failed to parse token amount".to_string()
                    ))?;
                let token_amount = u64::from_le_bytes(amount_bytes);
                
                if token_amount > 0 {
                    return Ok(format!(
                        "Account still holds {} tokens ({:.6} USDC). Must burn tokens first before account can be closed.",
                        token_amount,
                        token_amount as f64 / 1_000_000.0
                    ));
                }
            }
            
            // Check close authority AFTER confirming token balance is zero
            if !self.has_close_authority(&account)? {
                return Ok("Operator is not the close authority for this SPL Token account".to_string());
            }
        }
        
        // Check inactivity period
        let now = Utc::now();
        let min_inactive = Duration::days(self.config.reclaim.min_inactive_days as i64);
        let age = now - created_at;
        
        if age < min_inactive {
            let days_remaining = (min_inactive - age).num_days();
            return Ok(format!(
                "Account needs {} more days of inactivity (created {} days ago, requires {} days)",
                days_remaining,
                age.num_days(),
                self.config.reclaim.min_inactive_days
            ));
        }
        
        // Check recent activity
        let is_inactive = self.check_inactivity(pubkey).await.unwrap_or(false);
        if !is_inactive {
            return Ok("Account has recent transaction activity".to_string());
        }
        
        // Calculate rent and determine eligibility
        let min_balance = self.rpc_client.get_minimum_balance_for_rent_exemption(account.data.len())?;
        let is_empty = crate::solana::rent::RentCalculator::is_empty_account(&account, min_balance);
        
        if is_empty {
            return Ok(format!(
                "✓ ELIGIBLE: Empty SPL Token account with {} lamports ({:.8} SOL) rent-exempt balance",
                account.lamports,
                account.lamports as f64 / 1_000_000_000.0
            ));
        }
        
        if account.lamports <= min_balance * 2 {
            return Ok(format!(
                "✓ ELIGIBLE: SPL Token account with minimal balance of {} lamports ({:.8} SOL)",
                account.lamports,
                account.lamports as f64 / 1_000_000_000.0
            ));
        }
        
        Ok(format!(
            "Not eligible: account has significant balance ({} lamports, {:.8} SOL) beyond rent ({} lamports, {:.8} SOL). May contain user deposits.",
            account.lamports,
            account.lamports as f64 / 1_000_000_000.0,
            min_balance,
            min_balance as f64 / 1_000_000_000.0
        ))
    }
}