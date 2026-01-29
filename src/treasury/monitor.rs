// src/treasury/monitor.rs
use solana_sdk::pubkey::Pubkey;
//use chrono::{DateTime, Utc, Duration};
use crate::{
    error::Result,
    solana::client::SolanaRpcClient,
    storage::Database,
};
use tracing::{info, debug};

pub struct TreasuryMonitor {
    treasury_pubkey: Pubkey,
    rpc_client: SolanaRpcClient,
    db: Database,
}

impl TreasuryMonitor {
    pub fn new(
        treasury_pubkey: Pubkey,
        rpc_client: SolanaRpcClient,
        db: Database,
    ) -> Self {
        Self {
            treasury_pubkey,
            rpc_client,
            db,
        }
    }
    
    /// Monitor treasury balance and detect passive reclaims
    pub async fn check_for_passive_reclaims(&self) -> Result<Vec<super::reconciliation::PassiveReclaim>> {
        info!("Checking treasury balance for passive reclaims...");
        
        // Get current balance
        let current_balance = self.rpc_client.get_balance(&self.treasury_pubkey).await?;
        
        // Get last known balance from database
        let last_balance = self.db.get_last_treasury_balance()?;
        
        if current_balance <= last_balance {
            debug!("Treasury balance unchanged or decreased: {} -> {}", last_balance, current_balance);
            // Update balance even if decreased (might be used for operations)
            self.db.save_treasury_balance(current_balance)?;
            return Ok(vec![]);
        }
        
        let increase = current_balance - last_balance;
        info!("Treasury balance increased by {} lamports ({:.9} SOL)", 
            increase, 
            crate::solana::rent::RentCalculator::lamports_to_sol(increase)
        );
        
        // Find accounts that were recently closed and match this amount
        let passive_reclaims = self.correlate_balance_increase(increase).await?;
        
        // Update balance
        self.db.save_treasury_balance(current_balance)?;
        
        Ok(passive_reclaims)
    }
    
    /// Correlate balance increase with recently closed accounts
    /// Correlate balance increase with recently closed accounts
    async fn correlate_balance_increase(
        &self,
        increase: u64,
    ) -> Result<Vec<super::reconciliation::PassiveReclaim>> {
        // Get accounts that changed to Closed status recently (last 24 hours)
        let mut closed_accounts = self.db.get_recently_closed_accounts(24)?;
        
        // 1. Try to match with known closed accounts
        let mut matches = if !closed_accounts.is_empty() {
             debug!("Found {} recently closed accounts", closed_accounts.len());
             super::reconciliation::TreasuryReconciliation::match_amount_to_accounts(
                increase,
                &closed_accounts,
            )
        } else {
            info!("No recently closed accounts initially found");
            vec![]
        };

        // 2. If no high-confidence match, look for active accounts that might have closed
        // Check if we have a High confidence match
        let has_high_confidence = matches.iter().any(|m| matches!(m.confidence, super::reconciliation::ConfidenceLevel::High));
        
        if !has_high_confidence {
             // Search for ACTIVE accounts with rent close to 'increase'
             // Tolerance 5000 lamports (0.000005 SOL)
             let tolerance = 5000;
             let min = if increase > tolerance { increase - tolerance } else { 0 };
             let max = increase + tolerance;
             
             let candidates = self.db.get_active_accounts_by_rent_range(min, max)?;
             
             if !candidates.is_empty() {
                 info!("Found {} active candidates for possible attribution. Checking on-chain status...", candidates.len());
                 let mut found_closed = false;
                 
                 for candidate in candidates {
                     // Check if account still exists on-chain
                     if let Ok(pubkey) = candidate.pubkey.parse::<Pubkey>() {
                         // We don't have rate_limiter here, but candidate list should be small (filtered by amount)
                         if let Ok(account_opt) = self.rpc_client.get_account(&pubkey).await {
                             let is_closed = match account_opt {
                                 None => true, // Account gone
                                 Some(acc) => acc.lamports == 0, // Should be gone if 0
                             };
                             
                             if is_closed {
                                 info!("Account {} found closed on-chain! Marking as Closed.", candidate.pubkey);
                                 // Mark as closed in DB
                                 self.db.update_account_status(&candidate.pubkey, crate::storage::models::AccountStatus::Closed)?;
                                 self.db.update_account_authority(&candidate.pubkey, None, "PassiveMonitoring")?;
                                 
                                 // Add to closed_accounts list for matching
                                 closed_accounts.push(candidate);
                                 found_closed = true;
                             }
                         }
                     }
                 }
                 
                 if found_closed {
                     // Retry matching with updated list
                     debug!("Retrying correlation with newly discovered closed accounts");
                     matches = super::reconciliation::TreasuryReconciliation::match_amount_to_accounts(
                        increase,
                        &closed_accounts,
                     );
                 }
             }
        }
        
        Ok(matches)
    }
    
    /// Get total passive reclaims recorded
    pub fn get_total_passive_reclaimed(&self) -> Result<u64> {
        self.db.get_total_passive_reclaimed()
    }
}