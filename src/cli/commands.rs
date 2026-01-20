use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "kora-reclaim")]
#[command(about = "Automated rent reclaim bot for Kora-sponsored Solana accounts")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Launch interactive TUI dashboard 🆕
    Tui,
    
    /// Scan for eligible accounts
    Scan {
        /// Show detailed information
        #[arg(short, long)]
        verbose: bool,
    },
    
    /// Reclaim rent from specific account
    Reclaim {
        /// Account public key to reclaim
        pubkey: String,
        
        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },
    
    /// Run automated reclaim service
    Auto {
        /// Check interval in seconds
        #[arg(short, long, default_value = "3600")]
        interval: u64,
    },
    
    /// Show statistics and reports
    Stats,
    
    /// Initialize database and configuration
    Init,
}