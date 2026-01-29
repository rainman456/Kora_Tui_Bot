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
    pub creation_signature: Option<String>,
    pub creation_slot: Option<u64>,
    pub close_authority: Option<String>,
    pub reclaim_strategy: Option<ReclaimStrategy>,
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


// Add to src/storage/models.rs

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassiveReclaimRecord {
    pub id: i64,
    pub amount: u64,
    pub attributed_accounts: Vec<String>,
    pub confidence: String,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ReclaimStrategy {
    ActiveReclaim,      // Operator has close authority
    PassiveMonitoring,  // User controls, monitor for passive return
    Unrecoverable,      // Permanently locked (system accounts)
    Unknown,            // Not yet determined
}

impl std::fmt::Display for ReclaimStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReclaimStrategy::ActiveReclaim => write!(f, "ActiveReclaim"),
            ReclaimStrategy::PassiveMonitoring => write!(f, "PassiveMonitoring"),
            ReclaimStrategy::Unrecoverable => write!(f, "Unrecoverable"),
            ReclaimStrategy::Unknown => write!(f, "Unknown"),
        }
    }
}

impl std::str::FromStr for ReclaimStrategy {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "ActiveReclaim" => Ok(ReclaimStrategy::ActiveReclaim),
            "PassiveMonitoring" => Ok(ReclaimStrategy::PassiveMonitoring),
            "Unrecoverable" => Ok(ReclaimStrategy::Unrecoverable),
            _ => Ok(ReclaimStrategy::Unknown),
        }
    }
}


impl SponsoredAccount {
    #[allow(dead_code)]
    pub fn new(pubkey: Pubkey, rent_lamports: u64, data_size: usize) -> Self {
        Self {
            pubkey: pubkey.to_string(),
            created_at: Utc::now(),
            closed_at: None,
            rent_lamports,
            data_size,
            status: AccountStatus::Active,
            creation_signature: None,  
            creation_slot: None,
            close_authority: None,
            reclaim_strategy: None,
        }
    }
    
    #[allow(dead_code)]
    pub fn mark_closed(&mut self) {
        self.status = AccountStatus::Closed;
        self.closed_at = Some(Utc::now());
    }
    
    #[allow(dead_code)]
    pub fn mark_reclaimed(&mut self) {
        self.status = AccountStatus::Reclaimed;
    }
}