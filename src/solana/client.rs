use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    account::Account,
    commitment_config::CommitmentConfig,
    pubkey::Pubkey,
    signature::Signature,
    transaction::Transaction,
};
use solana_transaction_status::{
    UiTransactionEncoding, EncodedConfirmedTransactionWithStatusMeta,
};
use solana_client::rpc_config::{RpcTransactionConfig, RpcSignaturesForAddressConfig};
use crate::error::Result;
use tracing::{debug, warn};
use std::time::Duration;

#[derive(Clone)]
pub struct SolanaRpcClient {
    pub client: RpcClient,
    pub(crate) rate_limit_delay: Duration,
}

impl SolanaRpcClient {
    pub fn new(rpc_url: &str, commitment: CommitmentConfig, rate_limit_ms: u64) -> Self {
        let client = RpcClient::new_with_commitment(rpc_url.to_string(), commitment);
        let rate_limit_delay = Duration::from_millis(rate_limit_ms);
        Self { client, rate_limit_delay }
    }
    
    /// Apply rate limiting delay to avoid RPC throttling
    async fn rate_limit(&self) {
        tokio::time::sleep(self.rate_limit_delay).await;
    }
    
    /// Get account information
    pub async fn get_account(&self, pubkey: &Pubkey) -> Result<Option<Account>> {
        self.rate_limit().await;
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
    pub async fn is_account_active(&self, pubkey: &Pubkey) -> Result<bool> {
        Ok(self.get_account(pubkey).await?.is_some())
    }
    
    /// Get minimum balance for rent exemption
    pub fn get_minimum_balance_for_rent_exemption(&self, data_len: usize) -> Result<u64> {
        Ok(self.client.get_minimum_balance_for_rent_exemption(data_len)?)
    }
    
    /// Get account balance (lamports)
    pub async fn get_balance(&self, pubkey: &Pubkey) -> Result<u64> {
        self.rate_limit().await;
        Ok(self.client.get_balance(pubkey)?)
    }
    
    /// Get multiple accounts efficiently
    pub async fn get_multiple_accounts(&self, pubkeys: &[Pubkey]) -> Result<Vec<Option<Account>>> {
        self.rate_limit().await;
        Ok(self.client.get_multiple_accounts(pubkeys)?)
    }
    
    /// Get transaction signatures for an address with pagination
    /// Returns signatures in descending order (newest first)
    pub async fn get_signatures_for_address(
        &self,
        address: &Pubkey,
        before: Option<Signature>,
        until: Option<Signature>,
        limit: usize,
    ) -> Result<Vec<solana_client::rpc_response::RpcConfirmedTransactionStatusWithSignature>> {
        self.rate_limit().await;
        
        let config = RpcSignaturesForAddressConfig {
            before: before.map(|s| s.to_string()),
            until: until.map(|s| s.to_string()),
            limit: Some(limit),
            commitment: Some(self.client.commitment()),
            ..Default::default()
        };
        
        debug!("Fetching signatures for address: {}", address);
        let signatures = self.client.get_signatures_for_address_with_config(address, config)?;
        debug!("Found {} signatures", signatures.len());
        
        Ok(signatures)
    }
    
    /// Get full transaction details
    pub async fn get_transaction(
        &self,
        signature: &Signature,
    ) -> Result<Option<EncodedConfirmedTransactionWithStatusMeta>> {
        self.rate_limit().await;
        
        let config = RpcTransactionConfig {
            encoding: Some(UiTransactionEncoding::Json),
            commitment: Some(self.client.commitment()),
            max_supported_transaction_version: Some(0),
        };
        
        match self.client.get_transaction_with_config(signature, config) {
            Ok(tx) => Ok(Some(tx)),
            Err(e) => {
                if e.to_string().contains("not found") {
                    warn!("Transaction not found: {}", signature);
                    Ok(None)
                } else {
                    Err(e.into())
                }
            }
        }
    }
    
    /// Get latest blockhash
    pub fn get_latest_blockhash(&self) -> Result<solana_sdk::hash::Hash> {
        Ok(self.client.get_latest_blockhash()?)
    }
    
    /// Send and confirm transaction with retry logic
    pub async fn send_and_confirm_transaction(
        &self,
        transaction: &Transaction,
    ) -> Result<Signature> {
        const MAX_RETRIES: u32 = 3;
        let mut last_error = None;
        
        for attempt in 1..=MAX_RETRIES {
            self.rate_limit().await;
            
            match self.client.send_and_confirm_transaction(transaction) {
                Ok(signature) => {
                    debug!("Transaction confirmed: {}", signature);
                    return Ok(signature);
                }
                Err(e) => {
                    warn!("Transaction attempt {} failed: {}", attempt, e);
                    last_error = Some(e);
                    
                    if attempt < MAX_RETRIES {
                        // Exponential backoff
                        let delay = Duration::from_secs(2u64.pow(attempt));
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }
        
        Err(last_error.unwrap().into())
    }
}