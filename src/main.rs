mod solana;
mod kora;
mod reclaim;
mod storage;
mod tui;
mod cli;
mod config;
mod error;
mod utils;

use clap::Parser;
use cli::{Cli, Commands};
use config::Config;
use tracing::{info, error};
use colored::*;

// TUI imports
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("kora_reclaim=debug,info")
        .init();
    
    // Parse CLI arguments
    let cli = Cli::parse();
    
    // Load configuration
    let config = match Config::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };
    
    // Execute command
    let result = match cli.command {
        Commands::Tui => {
            run_tui(config).await
        }
        
        Commands::Scan { verbose, dry_run, limit } => {
            info!("Scanning for eligible accounts...");
            scan_accounts(&config, verbose, dry_run, limit).await
        }
        
        Commands::Reclaim { pubkey, yes, dry_run } => {
            info!("Reclaiming account: {}", pubkey);
            reclaim_account(&config, &pubkey, yes, dry_run).await
        }
        
        Commands::Auto { interval, dry_run } => {
            info!("Starting automated reclaim service (interval: {}s)", interval);
            run_auto_service(&config, interval, dry_run).await
        }
        
        Commands::Stats { format } => {
            info!("Generating statistics...");
            show_stats(&config, &format).await
        }
        
        Commands::Init => {
            info!("Initializing...");
            initialize(&config).await
        }
    };
    
    if let Err(e) = result {
        error!("{}", format!("Error: {}", e).red());
        std::process::exit(1);
    }
}

async fn run_tui(config: Config) -> error::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    
    // Create app
    let mut app = tui::app::App::new(config).await?;
    
    // Load initial data
    app.refresh_data().await?;
    app.add_log(tui::app::LogLevel::Info, "TUI started".to_string());
    
    // Create event handler
    let mut events = tui::event::EventHandler::new(std::time::Duration::from_millis(250));
    
    // Main loop
    loop {
        terminal.draw(|f| {
            tui::ui::render_ui(f, &mut app);
        })?;
        
        if let Some(event) = events.next().await {
            match event {
                tui::event::Event::Tick => {
                    // Background updates
                }
                tui::event::Event::Key(key) => {
                    use crossterm::event::{KeyCode, KeyModifiers};
                    
                    match (key.code, key.modifiers) {
                        (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => {
                            app.quit();
                        }
                        (KeyCode::Tab, KeyModifiers::NONE) => {
                            app.next_screen();
                        }
                        (KeyCode::BackTab, _) => {
                            app.previous_screen();
                        }
                        (KeyCode::Up, _) | (KeyCode::Char('k'), _) => {
                            app.select_previous_account();
                        }
                        (KeyCode::Down, _) | (KeyCode::Char('j'), _) => {
                            app.select_next_account();
                        }
                        (KeyCode::Char('r'), _) => {
                            app.add_log(tui::app::LogLevel::Info, "Refreshing data...".to_string());
                            app.refresh_data().await?;
                            app.add_log(tui::app::LogLevel::Success, "Data refreshed".to_string());
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        
        if app.should_quit {
            break;
        }
    }
    
    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    
    Ok(())
}

// ... rest of existing functions (scan_accounts, reclaim_account, etc.)
async fn scan_accounts(config: &Config, verbose: bool, dry_run: bool, limit: Option<usize>) -> error::Result<()> {
    println!("{}", "Scanning for eligible accounts...".cyan());
    
    // Initialize Solana client
    let rpc_client = solana::SolanaRpcClient::new(
        &config.solana.rpc_url,
        config.commitment_config(),
        config.solana.rate_limit_delay_ms,
    );
    
    // Initialize Kora monitor
    let operator_pubkey = config.operator_pubkey()?;
    let monitor = kora::KoraMonitor::new(rpc_client.clone(), operator_pubkey);
    
    // Discover sponsored accounts
    let max_txns = limit.unwrap_or(5000);
    info!("Discovering sponsored accounts from up to {} transactions", max_txns);
    let sponsored_accounts = monitor.get_sponsored_accounts(max_txns).await?;
    
    println!("Found {} sponsored accounts", sponsored_accounts.len());
    
    // Check eligibility
    let eligibility_checker = reclaim::EligibilityChecker::new(rpc_client.clone(), config.clone());
    
    let mut eligible = Vec::new();
    let mut total_reclaimable = 0u64;
    
    for account_info in &sponsored_accounts {
        let is_eligible = eligibility_checker.is_eligible(&account_info.pubkey, account_info.created_at).await?;
        
        if is_eligible {
            // Get current balance
            if let Ok(balance) = rpc_client.get_balance(&account_info.pubkey).await {
                total_reclaimable += balance;
                eligible.push((account_info.clone(), balance));
            }
        }
    }
    
    // Display results
    println!("\n{}", "=== Scan Results ===".cyan().bold());
    println!("Total Sponsored:    {}", sponsored_accounts.len());
    println!("Eligible for Reclaim: {} ✓", eligible.len().to_string().green());
    println!(
        "Total Reclaimable:   {}",
        utils::format_sol(total_reclaimable)
    );
    
    if verbose && !eligible.is_empty() {
        println!("\n{}", "Eligible Accounts:".yellow());
        utils::print_table_border(90);
        utils::print_table_row(
            &["Pubkey", "Balance", "Created", "Status"],
            &[44, 20, 20, 15],
        );
        utils::print_table_border(90);
        
        for (account, balance) in &eligible {
            utils::print_table_row(
                &[
                    &account.pubkey.to_string(),
                    &utils::format_sol(*balance),
                    &utils::format_timestamp(&account.created_at),
                    "Eligible",
                ],
                &[44, 20, 20, 15],
            );
        }
        utils::print_table_border(90);
    }
    
    if dry_run && !eligible.is_empty() {
        println!("\n{}", "DRY RUN: No transactions will be sent".yellow());
    }
    
    Ok(())
}

async fn reclaim_account(config: &Config, pubkey: &str, yes: bool, dry_run: bool) -> error::Result<()> {
    use solana_sdk::pubkey::Pubkey;
    use std::str::FromStr;
    
    println!("{}", format!("Reclaiming account: {}", pubkey).cyan());
    
    let account_pubkey = Pubkey::from_str(pubkey)
        .map_err(|e| error::ReclaimError::Other(anyhow::anyhow!("Invalid pubkey: {}", e)))?;
    
    // Initialize clients
    let rpc_client = solana::SolanaRpcClient::new(
        &config.solana.rpc_url,
        config.commitment_config(),
        config.solana.rate_limit_delay_ms,
    );
    
    // Check eligibility
    let eligibility_checker = reclaim::EligibilityChecker::new(rpc_client.clone(), config.clone());
    
    // Get account info to determine creation time (use current time as fallback)
    let created_at = chrono::Utc::now() - chrono::Duration::days(365); // Assume old enough
    
    let reason = eligibility_checker.get_eligibility_reason(&account_pubkey, created_at).await?;
    println!("Eligibility: {}", reason);
    
    let is_eligible = eligibility_checker.is_eligible(&account_pubkey, created_at).await?;
    if !is_eligible {
        return Err(error::ReclaimError::NotEligible(reason));
    }
    
    // Get account balance
    let balance = rpc_client.get_balance(&account_pubkey).await?;
    println!("Account balance: {}", utils::format_sol(balance));
    
    // Confirm action
    if !yes && !dry_run {
        if !utils::confirm_action(&format!("Reclaim {} from this account?", utils::format_sol(balance))) {
            println!("Cancelled");
            return Ok(());
        }
    }
    
    // Load treasury keypair
    let treasury_keypair = config.load_treasury_keypair()?;
    let treasury_wallet = config.treasury_wallet()?;
    
    // Initialize reclaim engine
    let engine = reclaim::ReclaimEngine::new(
        rpc_client,
        treasury_wallet,
        treasury_keypair,
        dry_run || config.reclaim.dry_run,
    );
    
    // Determine account type (default to System for now)
    let account_type = kora::AccountType::System;
    
    // Reclaim
    let result = engine.reclaim_account(&account_pubkey, &account_type).await?;
    
    if let Some(sig) = result.signature {
        println!("✓ Reclaim successful!");
        println!("Signature: {}", sig);
        println!("Reclaimed: {}", utils::format_sol(result.amount_reclaimed));
    } else if result.dry_run {
        println!("DRY RUN: Would reclaim {}", utils::format_sol(result.amount_reclaimed));
    }
    
    Ok(())
}

async fn run_auto_service(config: &Config, interval: u64, dry_run: bool) -> error::Result<()> {
    println!("{}", "Starting automated reclaim service...".green());
    println!("Interval: {} seconds", interval);
    println!("Dry run: {}", dry_run);
    
    let actual_dry_run = dry_run || config.reclaim.dry_run;
    
    loop {
        info!("Running reclaim cycle...");
        
        // Initialize clients
        let rpc_client = solana::SolanaRpcClient::new(
            &config.solana.rpc_url,
            config.commitment_config(),
            config.solana.rate_limit_delay_ms,
        );
        
        let operator_pubkey = config.operator_pubkey()?;
        let monitor = kora::KoraMonitor::new(rpc_client.clone(), operator_pubkey);
        
        // Discover accounts
        let sponsored_accounts = match monitor.get_sponsored_accounts(5000).await {
            Ok(accounts) => accounts,
            Err(e) => {
                warn!("Failed to discover accounts: {}", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;
                continue;
            }
        };
        
        info!("Found {} sponsored accounts", sponsored_accounts.len());
        
        // Check eligibility
        let eligibility_checker = reclaim::EligibilityChecker::new(rpc_client.clone(), config.clone());
        let mut eligible = Vec::new();
        
        for account_info in sponsored_accounts {
            if let Ok(true) = eligibility_checker.is_eligible(&account_info.pubkey, account_info.created_at).await {
                eligible.push((account_info.pubkey, account_info.account_type));
            }
        }
        
        if !eligible.is_empty() {
            info!("Found {} eligible accounts", eligible.len());
            
            // Load treasury and reclaim
            if let Ok(treasury_keypair) = config.load_treasury_keypair() {
                let treasury_wallet = config.treasury_wallet()?;
                let engine = reclaim::ReclaimEngine::new(
                    rpc_client.clone(),
                    treasury_wallet,
                    treasury_keypair,
                    actual_dry_run,
                );
                
                let batch_processor = reclaim::BatchProcessor::new(
                    engine,
                    config.reclaim.batch_size,
                    config.reclaim.batch_delay_ms,
                );
                
                match batch_processor.reclaim_all_eligible(eligible).await {
                    Ok(summary) => {
                        info!(
                            "Batch complete: {} successful, {} failed, {} SOL reclaimed",
                            summary.successful,
                            summary.failed,
                            solana::rent::RentCalculator::lamports_to_sol(summary.total_reclaimed)
                        );
                    }
                    Err(e) => {
                        warn!("Batch processing failed: {}", e);
                    }
                }
            }
        } else {
            info!("No eligible accounts found");
        }
        
        tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;
    }
}

async fn show_stats(config: &Config, format: &str) -> error::Result<()> {
    let db = storage::Database::new(&config.database.path)?;
    let stats = db.get_stats()?;
    
    if format == "json" {
        println!("{}", serde_json::to_string_pretty(&stats)?);
        return Ok(());
    }
    
    // Table format
    println!("{}", "=== Kora Rent Reclaim Statistics ===".cyan().bold());
    println!("\nAccounts:");
    println!("  Total:      {}", stats.total_accounts);
    println!("  Active:     {}", stats.active_accounts.to_string().green());
    println!("  Closed:     {}", stats.closed_accounts.to_string().yellow());
    println!("  Reclaimed:  {}", stats.reclaimed_accounts.to_string().cyan());
    
    println!("\nReclaim Operations:");
    println!("  Total:      {}", stats.total_operations);
    println!("  Total SOL:  {}", utils::format_sol(stats.total_reclaimed));
    println!("  Average:    {}", utils::format_sol(stats.avg_reclaim_amount));
    
    // Show recent history
    let history = db.get_reclaim_history(Some(10))?;
    if !history.is_empty() {
        println!("\n{}", "Recent Reclaim Operations:".yellow());
        utils::print_table_border(100);
        utils::print_table_row(
            &["Timestamp", "Account", "Amount", "Signature"],
            &[22, 44, 15, 20],
        );
        utils::print_table_border(100);
        
        for op in history {
            utils::print_table_row(
                &[
                    &utils::format_timestamp(&op.timestamp),
                    &utils::format_pubkey(&op.account_pubkey),
                    &utils::format_sol(op.reclaimed_amount),
                    &utils::format_pubkey(&op.tx_signature),
                ],
                &[22, 44, 15, 20],
            );
        }
        utils::print_table_border(100);
    }
    
    Ok(())
}

async fn initialize(config: &Config) -> error::Result<()> {
    println!("{}", "Initializing Kora Rent Reclaim Bot...".green());
    let db = storage::Database::new(&config.database.path)?;
    println!("{}", "✓ Database initialized".green());
    println!("{}", "✓ Configuration loaded".green());
    println!("\n{}", "Configuration:".cyan());
    println!("  RPC URL:        {}", config.solana.rpc_url);
    println!("  Network:        {:?}", config.solana.network);
    println!("  Operator:       {}", config.kora.operator_pubkey);
    println!("  Treasury:       {}", config.kora.treasury_wallet);
    println!("  Dry Run:        {}", config.reclaim.dry_run);
    println!("  Min Inactive:   {} days", config.reclaim.min_inactive_days);
    
    println!("\n{}", "Ready to use! Try running:".cyan());
    println!("  {} to scan for eligible accounts", "kora-reclaim scan --verbose".yellow());
    println!("  {} to view statistics", "kora-reclaim stats".yellow());
    println!("  {} to launch TUI dashboard", "kora-reclaim tui".yellow());
    Ok(())
}