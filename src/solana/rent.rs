use solana_sdk::account::Account;
use crate::error::Result;

pub struct RentCalculator;

impl RentCalculator {
    /// Calculate rent locked in an account
    pub fn calculate_rent(account: &Account) -> u64 {
        account.lamports
    }
    
    /// Calculate total rent across multiple accounts
    pub fn calculate_total_rent(accounts: &[(Account, String)]) -> u64 {
        accounts.iter()
            .map(|(account, _)| account.lamports)
            .sum()
    }
    
    /// Get account data size
    pub fn get_data_size(account: &Account) -> usize {
        account.data.len()
    }
    
    /// Check if account is rent-exempt
    pub fn is_rent_exempt(account: &Account, minimum_balance: u64) -> bool {
        account.lamports >= minimum_balance
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solana_sdk::account::Account;
    
    #[test]
    fn test_calculate_rent() {
        let account = Account {
            lamports: 1_000_000,
            data: vec![0; 100],
            owner: solana_sdk::system_program::id(),
            executable: false,
            rent_epoch: 0,
        };
        
        assert_eq!(RentCalculator::calculate_rent(&account), 1_000_000);
    }
}