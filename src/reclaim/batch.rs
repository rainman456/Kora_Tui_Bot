use solana_sdk::pubkey::Pubkey;
use crate::{
    error::Result,
    reclaim::engine::{ReclaimEngine, ReclaimResult},
    kora::types::AccountType,
};
use tracing::{info, warn};
use std::time::Duration;

/// Batch processor for reclaiming multiple accounts with rate limiting
pub struct BatchProcessor {
    engine: ReclaimEngine,
    batch_size: usize,
    batch_delay: Duration,
}

impl BatchProcessor {
    pub fn new(engine: ReclaimEngine, batch_size: usize, batch_delay_ms: u64) -> Self {
        Self {
            engine,
            batch_size,
            batch_delay: Duration::from_millis(batch_delay_ms),
        }
    }
    
    /// Process multiple accounts in batches with rate limiting
    pub async fn process_batch(
        &self,
        accounts: Vec<(Pubkey, AccountType)>,
    ) -> Result<BatchSummary> {
        info!(
            "Processing {} accounts in batches of {}",
            accounts.len(),
            self.batch_size
        );
        
        let mut summary = BatchSummary::default();
        summary.total_accounts = accounts.len();
        
        // Process in batches
        for (batch_num, chunk) in accounts.chunks(self.batch_size).enumerate() {
            info!("Processing batch {}/{}", batch_num + 1, (accounts.len() + self.batch_size - 1) / self.batch_size);
            
            let results = self.engine.batch_reclaim(chunk).await;
            
            for (pubkey, result) in match results {
                Ok(res) => res,
                Err(e) => {
                    warn!("Batch reclaim failed for chunk: {}", e);
                    continue;
                }
            } {
                match result {
                    Ok(res) => {
                        summary.successful += 1;
                        summary.total_reclaimed += res.amount_reclaimed;
                        summary.results.push((pubkey, Ok(res)));
                    }
                    Err(e) => {
                        summary.failed += 1;
                        warn!("Failed to reclaim {}: {}", pubkey, e);
                        summary.results.push((pubkey, Err(e)));
                    }
                }
            }
            
            // Delay between batches (except after last batch)
            if batch_num < (accounts.len() + self.batch_size - 1) / self.batch_size - 1 {
                tokio::time::sleep(self.batch_delay).await;
            }
        }
        
        info!(
            "Batch processing complete: {} successful, {} failed, {} SOL reclaimed",
            summary.successful,
            summary.failed,
            crate::solana::rent::RentCalculator::lamports_to_sol(summary.total_reclaimed)
        );
        
        Ok(summary)
    }
    
    /// Process all eligible accounts found by scanning
    pub async fn reclaim_all_eligible(
        &self,
        eligible_accounts: Vec<(Pubkey, AccountType)>,
    ) -> Result<BatchSummary> {
        if eligible_accounts.is_empty() {
            info!("No eligible accounts to reclaim");
            return Ok(BatchSummary::default());
        }
        
        info!("Found {} eligible accounts for reclaim", eligible_accounts.len());
        self.process_batch(eligible_accounts).await
    }
}

/// Summary of batch processing results
#[derive(Debug, Default)]
pub struct BatchSummary {
    pub total_accounts: usize,
    pub successful: usize,
    pub failed: usize,
    pub total_reclaimed: u64,
    pub results: Vec<(Pubkey, Result<ReclaimResult>)>,
}

impl BatchSummary {
    /// Print a formatted summary to console
    pub fn print_summary(&self) {
        println!("\n{}", "=== Reclaim Batch Summary ===".to_string());
        println!("Total Accounts:  {}", self.total_accounts);
        println!("Successful:      {} ✓", self.successful);
        println!("Failed:          {} ✗", self.failed);
        println!(
            "Total Reclaimed: {} lamports ({:.9} SOL)",
            self.total_reclaimed,
            crate::solana::rent::RentCalculator::lamports_to_sol(self.total_reclaimed)
        );
        println!("{}", "============================".to_string());
    }
    
    /// Get success rate as percentage
    pub fn success_rate(&self) -> f64 {
        if self.total_accounts == 0 {
            0.0
        } else {
            (self.successful as f64 / self.total_accounts as f64) * 100.0
        }
    }
}
