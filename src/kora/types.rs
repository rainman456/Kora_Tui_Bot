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
}

/// Type of account (determines how to close it)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AccountType {
    /// System program account (close with transfer)
    System,
    /// SPL Token account (close with spl_token::close_account)
    SplToken,
    /// Other program account (store program ID for reference)
    Other(Pubkey),
}

impl AccountType {
    /// Get the program ID for this account type
    pub fn program_id(&self) -> Pubkey {
        match self {
            AccountType::System => solana_sdk::system_program::id(),
            AccountType::SplToken => spl_token::id(),
            AccountType::Other(program_id) => *program_id,
        }
    }
}

impl From<crate::solana::accounts::AccountType> for AccountType {
    fn from(value: crate::solana::accounts::AccountType) -> Self {
        match value {
            crate::solana::accounts::AccountType::System => AccountType::System,
            crate::solana::accounts::AccountType::SplToken => AccountType::SplToken,
            crate::solana::accounts::AccountType::Other(program_id) => AccountType::Other(program_id),
        }
    }
}
