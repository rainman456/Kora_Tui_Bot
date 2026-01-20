use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SponsoredAccount {
    pub pubkey: String,
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub rent_lamports: u64,
    pub data_size: usize,
    pub status: AccountStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AccountStatus {
    Active,
    Closed,
    Reclaimed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReclaimOperation {
    pub id: i64,
    pub account_pubkey: String,
    pub reclaimed_amount: u64,
    pub tx_signature: String,
    pub timestamp: DateTime<Utc>,
    pub reason: String,
}

impl SponsoredAccount {
    pub fn new(pubkey: Pubkey, rent_lamports: u64, data_size: usize) -> Self {
        Self {
            pubkey: pubkey.to_string(),
            created_at: Utc::now(),
            closed_at: None,
            rent_lamports,
            data_size,
            status: AccountStatus::Active,
        }
    }
    
    pub fn mark_closed(&mut self) {
        self.status = AccountStatus::Closed;
        self.closed_at = Some(Utc::now());
    }
    
    pub fn mark_reclaimed(&mut self) {
        self.status = AccountStatus::Reclaimed;
    }
}