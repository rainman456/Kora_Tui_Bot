use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    account::Account,
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
};
use crate::error::Result;

pub struct SolanaRpcClient {
    client: RpcClient,
}

impl SolanaRpcClient {
    pub fn new(rpc_url: &str, commitment: CommitmentConfig) -> Self {
        let client = RpcClient::new_with_commitment(rpc_url.to_string(), commitment);
        Self { client }
    }
    
    /// Get account information
    pub fn get_account(&self, pubkey: &Pubkey) -> Result<Option<Account>> {
        match self.client.get_account(pubkey) {
            Ok(account) => Ok(Some(account)),
            Err(e) => {
                // Account not found is not an error
                if e.to_string().contains("AccountNotFound") {
                    Ok(None)
                } else {
                    Err(e.into())
                }
            }
        }
    }
    
    /// Check if account exists and is active
    pub fn is_account_active(&self, pubkey: &Pubkey) -> Result<bool> {
        Ok(self.get_account(pubkey)?.is_some())
    }
    
    /// Get minimum balance for rent exemption
    pub fn get_minimum_balance_for_rent_exemption(&self, data_len: usize) -> Result<u64> {
        Ok(self.client.get_minimum_balance_for_rent_exemption(data_len)?)
    }
    
    /// Get account balance (lamports)
    pub fn get_balance(&self, pubkey: &Pubkey) -> Result<u64> {
        Ok(self.client.get_balance(pubkey)?)
    }
    
    /// Get multiple accounts efficiently
    pub fn get_multiple_accounts(&self, pubkeys: &[Pubkey]) -> Result<Vec<Option<Account>>> {
        Ok(self.client.get_multiple_accounts(pubkeys)?)
    }
}