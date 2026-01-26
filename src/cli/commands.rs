use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "kora-reclaim")]
#[command(about = "Automated rent reclaim bot for Kora-sponsored Solana accounts")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
    
    /// Path to configuration file
    #[arg(short, long, global = true, default_value = "config.toml")]
    pub config: String,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Launch interactive TUI dashboard
    Tui,
    
    /// Scan for eligible accounts
    Scan {
        /// Show detailed information
        #[arg(short, long)]
        verbose: bool,
        
        /// Dry run mode (don't actually reclaim)
        #[arg(long)]
        dry_run: bool,
        
        /// Limit number of accounts to scan
        #[arg(short, long)]
        limit: Option<usize>,
    },
    
    /// Reclaim rent from specific account
    Reclaim {
        /// Account public key to reclaim
        pubkey: String,
        
        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
        
        /// Dry run mode (simulate without sending transactions)
        #[arg(long)]
        dry_run: bool,
    },
    
    /// Run automated reclaim service
    Auto {
        /// Check interval in seconds
        #[arg(short, long, default_value = "3600")]
        interval: u64,
        
        /// Dry run mode (don't actually reclaim)
        #[arg(long)]
        dry_run: bool,
    },
    List {
        /// Filter by status (active, closed, reclaimed, all)
        #[arg(short, long, default_value = "all")]
        status: String,
        
        /// Output format (table, json)
        #[arg(short, long, default_value = "table")]
        format: String,
        
        /// Show detailed information including creation details
        #[arg(short, long)]
        detailed: bool,
    },
    
    /// Reset scanning checkpoints (force full rescan on next run)
    Reset {
        /// Skip confirmation prompt
        #[arg(short, long)]
        yes: bool,
    },
    
    /// Show checkpoint information and scanning state
    Checkpoints,
    
    
    /// Show statistics and reports
    Stats {
        /// Output format: table or json
        #[arg(short, long, default_value = "table")]
        format: String,
    },
    
    /// Initialize database and configuration
    Init,

    /// Start Telegram bot interface
    Telegram,
}