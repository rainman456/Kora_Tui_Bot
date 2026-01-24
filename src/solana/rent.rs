use solana_sdk::account::Account;

/// Lamports per SOL constant
pub const LAMPORTS_PER_SOL: u64 = 1_000_000_000;

pub struct RentCalculator;

impl RentCalculator {
    /// Calculate rent locked in an account
    #[allow(dead_code)]
    pub fn calculate_rent(account: &Account) -> u64 {
        account.lamports
    }
    
    /// Calculate total rent across multiple accounts
    #[allow(dead_code)]
    pub fn calculate_total_rent(accounts: &[(Account, String)]) -> u64 {
        accounts.iter()
            .map(|(account, _)| account.lamports)
            .sum()
    }
    
    /// Get account data size
    #[allow(dead_code)]
    pub fn get_data_size(account: &Account) -> usize {
        account.data.len()
    }
    
    /// Check if account is rent-exempt
    #[allow(dead_code)]
    pub fn is_rent_exempt(account: &Account, minimum_balance: u64) -> bool {
        account.lamports >= minimum_balance
    }
    
    /// Check if account is "empty" (only has rent-exempt minimum, no actual data)
    pub fn is_empty_account(account: &Account, minimum_balance: u64) -> bool {
        // Account has no data beyond allocation or balance is close to minimum
        account.data.is_empty() || 
        (account.lamports <= minimum_balance && account.data.iter().all(|&b| b == 0))
    }
    
    /// Convert lamports to SOL (as f64)
    pub fn lamports_to_sol(lamports: u64) -> f64 {
        lamports as f64 / LAMPORTS_PER_SOL as f64
    }
    
    /// Convert SOL to lamports
    #[allow(dead_code)]
    pub fn sol_to_lamports(sol: f64) -> u64 {
        (sol * LAMPORTS_PER_SOL as f64) as u64
    }
    
    /// Format lamports as SOL string with decimals
    #[allow(dead_code)]
    pub fn format_sol(lamports: u64) -> String {
        format!("{:.9} SOL", Self::lamports_to_sol(lamports))
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
    
    #[test]
    fn test_lamports_conversion() {
        assert_eq!(RentCalculator::lamports_to_sol(LAMPORTS_PER_SOL), 1.0);
        assert_eq!(RentCalculator::sol_to_lamports(1.0), LAMPORTS_PER_SOL);
    }
    
    #[test]
    fn test_is_empty_account() {
        let empty = Account {
            lamports: 1000,
            data: vec![],
            owner: solana_sdk::system_program::id(),
            executable: false,
            rent_epoch: 0,
        };
        assert!(RentCalculator::is_empty_account(&empty, 1000));
        
        let non_empty = Account {
            lamports: 1000,
            data: vec![1, 2, 3],
            owner: solana_sdk::system_program::id(),
            executable: false,
            rent_epoch: 0,
        };
        assert!(!RentCalculator::is_empty_account(&non_empty, 1000));
    }
}