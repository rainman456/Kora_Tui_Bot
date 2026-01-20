use serde::Deserialize;
use solana_sdk::{pubkey::Pubkey, signature::Keypair};
use std::str::FromStr;
use std::fs;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub solana: SolanaConfig,
    pub kora: KoraConfig,
    pub reclaim: ReclaimConfig,
    pub database: DatabaseConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SolanaConfig {
    pub rpc_url: String,
    pub network: Network,
    pub commitment: String,
    #[serde(default = "default_rate_limit")]
    pub rate_limit_delay_ms: u64,
}

fn default_rate_limit() -> u64 {
    100
}

#[derive(Debug, Deserialize, Clone)]
pub enum Network {
    Mainnet,
    Devnet,
    Testnet,
}

#[derive(Debug, Deserialize, Clone)]
pub struct KoraConfig {
    pub operator_pubkey: String,
    pub treasury_wallet: String,
    #[serde(default = "default_keypair_path")]
    pub treasury_keypair_path: String,
}

fn default_keypair_path() -> String {
    "./treasury-keypair.json".to_string()
}

#[derive(Debug, Deserialize, Clone)]
pub struct ReclaimConfig {
    pub min_inactive_days: u64,
    #[serde(default)]
    pub auto_reclaim_enabled: bool,
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    #[serde(default = "default_batch_delay")]
    pub batch_delay_ms: u64,
    #[serde(default = "default_scan_interval")]
    pub scan_interval_seconds: u64,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub whitelist: Vec<String>,
    #[serde(default)]
    pub blacklist: Vec<String>,
}

fn default_batch_size() -> usize {
    10
}

fn default_batch_delay() -> u64 {
    1000
}

fn default_scan_interval() -> u64 {
    3600
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub path: String,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        dotenv::dotenv().ok();
        
        let config = config::Config::builder()
            .add_source(config::File::with_name("config"))
            .add_source(config::Environment::with_prefix("KORA"))
            .build()?;
        
        Ok(config.try_deserialize()?)
    }
    
    pub fn operator_pubkey(&self) -> anyhow::Result<Pubkey> {
        Pubkey::from_str(&self.kora.operator_pubkey)
            .map_err(|e| anyhow::anyhow!("Invalid operator pubkey: {}", e))
    }
    
    pub fn treasury_wallet(&self) -> anyhow::Result<Pubkey> {
        Pubkey::from_str(&self.kora.treasury_wallet)
            .map_err(|e| anyhow::anyhow!("Invalid treasury wallet: {}", e))
    }
    
    /// Load treasury keypair from file
    pub fn load_treasury_keypair(&self) -> anyhow::Result<Keypair> {
        let keypair_bytes = fs::read(&self.kora.treasury_keypair_path)
            .map_err(|e| anyhow::anyhow!("Failed to read keypair file: {}", e))?;
        
        let keypair: Vec<u8> = serde_json::from_slice(&keypair_bytes)
            .map_err(|e| anyhow::anyhow!("Failed to parse keypair JSON: {}", e))?;
        
        Keypair::from_bytes(&keypair)
            .map_err(|e| anyhow::anyhow!("Invalid keypair bytes: {}", e))
    }
    
    /// Get Solana commitment config
    pub fn commitment_config(&self) -> solana_sdk::commitment_config::CommitmentConfig {
        use solana_sdk::commitment_config::{CommitmentConfig, CommitmentLevel};
        
        let level = match self.solana.commitment.to_lowercase().as_str() {
            "processed" => CommitmentLevel::Processed,
            "confirmed" => CommitmentLevel::Confirmed,
            "finalized" => CommitmentLevel::Finalized,
            _ => CommitmentLevel::Confirmed, // Default
        };
        
        CommitmentConfig { commitment: level }
    }
}