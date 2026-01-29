// src/treasury/reconciliation.rs
use solana_sdk::pubkey::Pubkey;
use chrono::{DateTime, Utc};
use crate::storage::models::SponsoredAccount;
use tracing::debug;

#[derive(Debug, Clone)]
pub struct PassiveReclaim {
    pub amount: u64,
    pub timestamp: DateTime<Utc>,
    pub attributed_accounts: Vec<Pubkey>,
    pub confidence: ConfidenceLevel,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfidenceLevel {
    High,      // Exact match to single account
    Medium,    // Match to 2-3 accounts
    Low,       // Match to multiple accounts or partial match
    Unknown,   // Can't correlate
}

pub struct TreasuryReconciliation;

impl TreasuryReconciliation {
    /// Match a balance increase to recently closed accounts
    pub fn match_amount_to_accounts(
        increase: u64,
        closed_accounts: &[SponsoredAccount],
    ) -> Vec<PassiveReclaim> {
        let mut reclaims = Vec::new();
        let tolerance = 5000u64; // Allow 5000 lamports tolerance for fees
        
        // Try to find exact single account match
        for account in closed_accounts {
            let diff = if increase > account.rent_lamports {
                increase - account.rent_lamports
            } else {
                account.rent_lamports - increase
            };
            
            if diff <= tolerance {
                debug!(
                    "High confidence match: {} lamports to account {} (diff: {})",
                    increase, account.pubkey, diff
                );
                
                let pubkey = account.pubkey.parse().unwrap_or_else(|_| Pubkey::default());
                
                reclaims.push(PassiveReclaim {
                    amount: increase,
                    timestamp: Utc::now(),
                    attributed_accounts: vec![pubkey],
                    confidence: ConfidenceLevel::High,
                });
                return reclaims;
            }
        }
        
        // Try to find combination of 2-3 accounts
        if closed_accounts.len() >= 2 {
            let combination = Self::find_account_combination(increase, closed_accounts, tolerance);
            if let Some((accounts, total)) = combination {
                debug!(
                    "Medium confidence match: {} lamports to {} accounts (total: {})",
                    increase, accounts.len(), total
                );
                
                reclaims.push(PassiveReclaim {
                    amount: increase,
                    timestamp: Utc::now(),
                    attributed_accounts: accounts,
                    confidence: ConfidenceLevel::Medium,
                });
                return reclaims;
            }
        }
        
        // Low confidence: record increase but can't match precisely
        debug!("Low confidence: {} lamports increase, {} closed accounts available", 
            increase, closed_accounts.len());
        
        let likely_accounts: Vec<Pubkey> = closed_accounts
            .iter()
            .take(5) // Take up to 5 most recent
            .filter_map(|acc| acc.pubkey.parse().ok())
            .collect();
        
        reclaims.push(PassiveReclaim {
            amount: increase,
            timestamp: Utc::now(),
            attributed_accounts: likely_accounts,
            confidence: if closed_accounts.is_empty() {
                ConfidenceLevel::Unknown
            } else {
                ConfidenceLevel::Low
            },
        });
        
        reclaims
    }
    
    /// Find combination of accounts that sum to the target amount
    fn find_account_combination(
        target: u64,
        accounts: &[SponsoredAccount],
        tolerance: u64,
    ) -> Option<(Vec<Pubkey>, u64)> {
        // Try pairs
        for i in 0..accounts.len() {
            for j in (i + 1)..accounts.len() {
                let sum = accounts[i].rent_lamports + accounts[j].rent_lamports;
                let diff = if sum > target { sum - target } else { target - sum };
                
                if diff <= tolerance {
                    let pubkeys = vec![
                        accounts[i].pubkey.parse().ok()?,
                        accounts[j].pubkey.parse().ok()?,
                    ];
                    return Some((pubkeys, sum));
                }
            }
        }
        
        // Try triplets
        for i in 0..accounts.len() {
            for j in (i + 1)..accounts.len() {
                for k in (j + 1)..accounts.len() {
                    let sum = accounts[i].rent_lamports 
                        + accounts[j].rent_lamports 
                        + accounts[k].rent_lamports;
                    let diff = if sum > target { sum - target } else { target - sum };
                    
                    if diff <= tolerance {
                        let pubkeys = vec![
                            accounts[i].pubkey.parse().ok()?,
                            accounts[j].pubkey.parse().ok()?,
                            accounts[k].pubkey.parse().ok()?,
                        ];
                        return Some((pubkeys, sum));
                    }
                }
            }
        }
        
        None
    }
}