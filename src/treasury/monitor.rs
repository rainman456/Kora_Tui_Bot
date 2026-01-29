// src/treasury/monitor.rs
use solana_sdk::pubkey::Pubkey;
use chrono::{DateTime, Utc, Duration};
use crate::{
    error::Result,
    solana::client::SolanaRpcClient,
    storage::Database,
};
use tracing::{info, debug, warn};

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
    async fn correlate_balance_increase(
        &self,
        increase: u64,
    ) -> Result<Vec<super::reconciliation::PassiveReclaim>> {
        // Get accounts that changed to Closed status recently (last 24 hours)
        let recently_closed = self.db.get_recently_closed_accounts(24)?;
        
        if recently_closed.is_empty() {
            info!("No recently closed accounts to correlate with balance increase");
            return Ok(vec![]);
        }
        
        debug!("Found {} recently closed accounts", recently_closed.len());
        
        // Try to match the increase amount with account balances
        let matches = super::reconciliation::TreasuryReconciliation::match_amount_to_accounts(
            increase,
            &recently_closed,
        );
        
        Ok(matches)
    }
    
    /// Get total passive reclaims recorded
    pub fn get_total_passive_reclaimed(&self) -> Result<u64> {
        self.db.get_total_passive_reclaimed()
    }
}