use thiserror::Error;

#[derive(Error, Debug)]
pub enum ReclaimError {
    #[error("Solana RPC error: {0}")]
    SolanaRpc(#[from] solana_client::client_error::ClientError),
    
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    
    #[error("Account not found: {0}")]
    AccountNotFound(String),
    
    #[error("Account not eligible for reclaim: {0}")]
    NotEligible(String),
    
    #[error("Invalid configuration: {0}")]
    Config(String),
    
    #[error("Transaction failed: {0}")]
    TransactionFailed(String),
    
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    
    #[error("Parse signature error: {0}")]
    ParseSignature(#[from] solana_sdk::signature::ParseSignatureError),
    
    #[error("Parse pubkey error: {0}")]
    ParsePubkey(#[from] solana_sdk::pubkey::ParsePubkeyError),
    
    #[error("Program error: {0}")]
    ProgramError(#[from] solana_sdk::program_error::ProgramError),
    
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

pub type Result<T> = std::result::Result<T, ReclaimError>;