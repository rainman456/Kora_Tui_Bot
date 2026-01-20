use serde::Deserialize;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

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
}

#[derive(Debug, Deserialize, Clone)]
pub struct ReclaimConfig {
    pub min_inactive_days: u64,
    pub auto_reclaim_enabled: bool,
    pub batch_size: usize,
    pub whitelist: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub path: String,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        dotenv::dotenv().ok();
        
        let config = config::Config::builder()
            .add_source(config::File::with_name("config/default"))
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
}