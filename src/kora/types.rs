// use solana_sdk::pubkey::Pubkey;
// use serde::{Deserialize, Serialize};
// use chrono::{DateTime, Utc};

// /// Information about a Kora-sponsored account
// #[derive(Debug, Clone, Serialize, Deserialize)]
// pub struct SponsoredAccountInfo {
//     pub pubkey: Pubkey,
//     pub created_at: DateTime<Utc>,
//     pub rent_lamports: u64,
//     pub data_size: usize,
//     pub account_type: AccountType,
//     pub last_activity: Option<DateTime<Utc>>,
//     pub creation_signature: solana_sdk::signature::Signature,
//     pub creation_slot: u64,
// }

// /// Type of account (determines how to close it)
// #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
// pub enum AccountType {
//     /// System program account (close with transfer)
//     System,
//     /// SPL Token account (close with spl_token::close_account)
//     SplToken,
//     /// Other program account (store program ID for reference)
//     Other(Pubkey),
// }

// impl AccountType {
//     /// Get the program ID for this account type
//     pub fn program_id(&self) -> Pubkey {
//         match self {
//             AccountType::System => solana_sdk::system_program::id(),
//             AccountType::SplToken => spl_token::id(),
//             AccountType::Other(program_id) => *program_id,
//         }
//     }
// }

// impl From<crate::solana::accounts::AccountType> for AccountType {
//     fn from(value: crate::solana::accounts::AccountType) -> Self {
//         match value {
//             crate::solana::accounts::AccountType::System => AccountType::System,
//             crate::solana::accounts::AccountType::SplToken => AccountType::SplToken,
//             crate::solana::accounts::AccountType::Other(program_id) => AccountType::Other(program_id),
//         }
//     }
// }





use solana_sdk::pubkey::Pubkey;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

/// Information about a Kora-sponsored account
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SponsoredAccountInfo {
    pub pubkey: Pubkey,
    pub created_at: DateTime<Utc>,
    pub rent_lamports: u64,
    pub data_size: usize,
    pub account_type: AccountType,
    pub last_activity: Option<DateTime<Utc>>,
    pub creation_signature: solana_sdk::signature::Signature,
    pub creation_slot: u64,
}

/// Type of account (determines how to close it)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AccountType {
    /// System program account (close with transfer)
    System,
    /// SPL Token account (close with spl_token::close_account)
    SplToken,
    /// SPL Token Mint account (cannot be closed - permanent infrastructure)
    SplMint,
    /// Other program account (store program ID for reference)
    Other(Pubkey),
}


use std::fmt;

impl fmt::Display for AccountType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AccountType::System => write!(f, "System"),
            AccountType::SplToken => write!(f, "SPL Token"),
            AccountType::SplMint => write!(f, "SPL Mint"),
            AccountType::Other(pubkey) => write!(f, "Other({})", pubkey),
        }
    }
}

impl AccountType {
    /// Get the program ID for this account type
    pub fn program_id(&self) -> Pubkey {
        match self {
            AccountType::System => solana_sdk::system_program::id(),
            AccountType::SplToken => spl_token::id(),
            AccountType::SplMint => spl_token::id(),
            AccountType::Other(program_id) => *program_id,
        }
    }
    
    /// Check if this account type is reclaimable
    pub fn is_reclaimable(&self) -> bool {
        matches!(self, AccountType::SplToken)
    }
    
    /// Get a human-readable description of the account type
    pub fn description(&self) -> &str {
        match self {
            AccountType::System => "System account (user-owned)",
            AccountType::SplToken => "SPL Token account",
            AccountType::SplMint => "SPL Token Mint account",
            AccountType::Other(_) => "Custom program account",
        }
    }
}

impl From<crate::solana::accounts::AccountType> for AccountType {
    fn from(value: crate::solana::accounts::AccountType) -> Self {
        match value {
            crate::solana::accounts::AccountType::System => AccountType::System,
            crate::solana::accounts::AccountType::SplToken => AccountType::SplToken,
            crate::solana::accounts::AccountType::SplMint => AccountType::SplMint,
            crate::solana::accounts::AccountType::Other(program_id) => AccountType::Other(program_id),
        }
    }
}