mod solana;
mod kora;
mod reclaim;
mod storage;
mod cli;
mod config;
mod error;
mod utils;
mod telegram;

use clap::Parser;
use cli::{Cli, Commands};
use config::Config;
use tracing::{info, error, warn};
use colored::*;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter("kora_reclaim=debug,info")
        .init();
    
    let cli = Cli::parse();
    
    let config = match Config::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };
    
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
        
        Commands::Telegram => {
            info!("Starting Telegram bot interface...");
            telegram::run_telegram_bot(config).await
        }
    };
    
    if let Err(e) = result {
        error!("{}", format!("Error: {}", e).red());
        std::process::exit(1);
    }
}

async fn run_tui(_config: Config) -> error::Result<()> {
    println!("{}", "TUI not implemented yet - backend only".yellow());
    println!("Use CLI commands instead:");
    println!("  {} - Scan for eligible accounts", "kora-reclaim scan --verbose".cyan());
    println!("  {} - View statistics", "kora-reclaim stats".cyan());
    println!("  {} - Run automated service", "kora-reclaim auto".cyan());
    Ok(())
}


async fn scan_accounts(config: &Config, verbose: bool, dry_run: bool, limit: Option<usize>) -> error::Result<()> {
    println!("{}", "Scanning for eligible accounts...".cyan());
    
    let rpc_client = solana::SolanaRpcClient::new(
        &config.solana.rpc_url,
        config.commitment_config(),
        config.solana.rate_limit_delay_ms,
    );
    
    let operator_pubkey = config.operator_pubkey()?;
    let monitor = kora::KoraMonitor::new(rpc_client.clone(), operator_pubkey);
    
    let max_txns = limit.unwrap_or(5000);
    info!("Discovering sponsored accounts from up to {} transactions", max_txns);
    let sponsored_accounts = monitor.get_sponsored_accounts(max_txns).await?;
    
    println!("Found {} sponsored accounts", sponsored_accounts.len());
    
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
        rpc_client.clone(),
        treasury_wallet,
        treasury_keypair,
        dry_run || config.reclaim.dry_run,
    );
    
    // Determine account type - Default to SplToken since System accounts can't be reclaimed
    // In production, you should detect the actual account type
    let account_type = kora::AccountType::SplToken;
    
    // Reclaim
    let result = engine.reclaim_account(&account_pubkey, &account_type).await?;
    
    if let Some(sig) = result.signature {
        println!("✓ Reclaim successful!");
        println!("Signature: {}", sig);
        println!("Reclaimed: {}", utils::format_sol(result.amount_reclaimed));
        
        // Save to database
        let db = storage::Database::new(&config.database.path)?;
        
        db.update_account_status(&pubkey, storage::models::AccountStatus::Reclaimed)?;
        
        db.save_reclaim_operation(&storage::models::ReclaimOperation {
            id: 0, // Will be auto-generated
            account_pubkey: pubkey.to_string(),
            reclaimed_amount: result.amount_reclaimed,
            tx_signature: sig.to_string(),
            timestamp: chrono::Utc::now(),
            reason: "Manual CLI reclaim".to_string(),
        })?;
        
        info!("Reclaim operation saved to database");
        
        // Send notification if enabled
        if let Some(notifier) = telegram::AutoNotifier::new(config) {
            notifier.notify_reclaim_success(&pubkey, result.amount_reclaimed).await;
        }
        
    } else if result.dry_run {
        println!("DRY RUN: Would reclaim {}", utils::format_sol(result.amount_reclaimed));
    }
    
    Ok(())
}

// async fn run_auto_service(config: &Config, interval: u64, dry_run: bool) -> error::Result<()> {
//     println!("{}", "Starting automated reclaim service...".green());
//     println!("Interval: {} seconds", interval);
//     println!("Dry run: {}", dry_run);
    
//     let actual_dry_run = dry_run || config.reclaim.dry_run;
    
//     loop {
//         info!("Running reclaim cycle...");
        
//         // Initialize clients
//         let rpc_client = solana::SolanaRpcClient::new(
//             &config.solana.rpc_url,
//             config.commitment_config(),
//             config.solana.rate_limit_delay_ms,
//         );
        
//         let operator_pubkey = config.operator_pubkey()?;
//         let monitor = kora::KoraMonitor::new(rpc_client.clone(), operator_pubkey);
        
//         // Discover accounts
//         let sponsored_accounts = match monitor.get_sponsored_accounts(5000).await {
//             Ok(accounts) => accounts,
//             Err(e) => {
//                 warn!("Failed to discover accounts: {}", e);
//                 tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;
//                 continue;
//             }
//         };
        
//         info!("Found {} sponsored accounts", sponsored_accounts.len());
        
//         // Check eligibility
//         let eligibility_checker = reclaim::EligibilityChecker::new(rpc_client.clone(), config.clone());
//         let mut eligible = Vec::new();
        
//         for account_info in sponsored_accounts {
//             if let Ok(true) = eligibility_checker.is_eligible(&account_info.pubkey, account_info.created_at).await {
//                 eligible.push((account_info.pubkey, account_info.account_type));
//             }
//         }
        
//         if !eligible.is_empty() {
//             info!("Found {} eligible accounts", eligible.len());
            
//             // Load treasury and reclaim
//             if let Ok(treasury_keypair) = config.load_treasury_keypair() {
//                 let treasury_wallet = config.treasury_wallet()?;
//                 let engine = reclaim::ReclaimEngine::new(
//                     rpc_client.clone(),
//                     treasury_wallet,
//                     treasury_keypair,
//                     actual_dry_run,
//                 );
                
//                 let batch_processor = reclaim::BatchProcessor::new(
//                     engine,
//                     config.reclaim.batch_size,
//                     config.reclaim.batch_delay_ms,
//                 );
                
//                 match batch_processor.reclaim_all_eligible(eligible).await {
//                     Ok(summary) => {
//                         info!(
//                             "Batch complete: {} successful, {} failed, {} SOL reclaimed",
//                             summary.successful,
//                             summary.failed,
//                             solana::rent::RentCalculator::lamports_to_sol(summary.total_reclaimed)
//                         );
//                     }
//                     Err(e) => {
//                         warn!("Batch processing failed: {}", e);
//                     }
//                 }
//             }
//         } else {
//             info!("No eligible accounts found");
//         }
        
//         tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;
//     }
// }


async fn run_auto_service(config: &Config, interval: u64, dry_run: bool) -> error::Result<()> {
    println!("{}", "Starting automated reclaim service...".green());
    println!("Interval: {} seconds", interval);
    println!("Dry run: {}", dry_run);
    
    let actual_dry_run = dry_run || config.reclaim.dry_run;
    
    // Initialize auto-notifier
    let notifier = telegram::AutoNotifier::new(config);
    if notifier.is_some() {
        println!("{}", "✓ Telegram notifications enabled".green());
    }
    
    loop {
        info!("Running reclaim cycle...");
        
        // Initialize clients
        let rpc_client = solana::SolanaRpcClient::new(
            &config.solana.rpc_url,
            config.commitment_config(),
            config.solana.rate_limit_delay_ms,
        );
        
        let operator_pubkey = match config.operator_pubkey() {
            Ok(pk) => pk,
            Err(e) => {
                error!("Failed to get operator pubkey: {}", e);
                if let Some(ref n) = notifier {
                    n.notify_error(&format!("Failed to get operator pubkey: {}", e)).await;
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;
                continue;
            }
        };
        
        let monitor = kora::KoraMonitor::new(rpc_client.clone(), operator_pubkey);
        
        // Discover accounts
        let sponsored_accounts = match monitor.get_sponsored_accounts(5000).await {
            Ok(accounts) => accounts,
            Err(e) => {
                warn!("Failed to discover accounts: {}", e);
                if let Some(ref n) = notifier {
                    n.notify_error(&format!("Account discovery failed: {}", e)).await;
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;
                continue;
            }
        };
        
        info!("Found {} sponsored accounts", sponsored_accounts.len());
        
        // Check eligibility
        let eligibility_checker = reclaim::EligibilityChecker::new(rpc_client.clone(), config.clone());
        let mut eligible = Vec::new();
        
        for account_info in &sponsored_accounts {
            if let Ok(true) = eligibility_checker.is_eligible(&account_info.pubkey, account_info.created_at).await {
                eligible.push((account_info.pubkey, account_info.account_type.clone()));
            }
        }
        
        // Notify scan complete
        if let Some(ref n) = notifier {
            n.notify_scan_complete(sponsored_accounts.len(), eligible.len()).await;
        }
        
        if !eligible.is_empty() {
            info!("Found {} eligible accounts", eligible.len());
            
            // Load treasury and reclaim
            let treasury_keypair = match config.load_treasury_keypair() {
                Ok(kp) => kp,
                Err(e) => {
                    error!("Failed to load treasury keypair: {}", e);
                    if let Some(ref n) = notifier {
                        n.notify_error(&format!("Failed to load treasury keypair: {}", e)).await;
                    }
                    tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;
                    continue;
                }
            };
            
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
                    
                    // Save successful reclaims to database and send notifications
                    if summary.successful > 0 {
                        if let Ok(db) = storage::Database::new(&config.database.path) {
                            for (pubkey, result) in &summary.results {
                                if let Ok(reclaim_result) = result {
                                    if let Some(sig) = reclaim_result.signature {
                                        // Update account status
                                        let _ = db.update_account_status(
                                            &pubkey.to_string(),
                                            storage::models::AccountStatus::Reclaimed
                                        );
                                        
                                        // Save reclaim operation
                                        let _ = db.save_reclaim_operation(&storage::models::ReclaimOperation {
                                            id: 0,
                                            account_pubkey: pubkey.to_string(),
                                            reclaimed_amount: reclaim_result.amount_reclaimed,
                                            tx_signature: sig.to_string(),
                                            timestamp: chrono::Utc::now(),
                                            reason: "Automated batch reclaim".to_string(),
                                        });
                                        
                                        // Send individual success notification for high-value reclaims
                                        if let Some(ref n) = notifier {
                                            if let Some(tg_config) = &config.telegram {
                                                n.notify_high_value_reclaim(
                                                    &pubkey.to_string(),
                                                    reclaim_result.amount_reclaimed,
                                                    tg_config.alert_threshold_sol
                                                ).await;
                                            }
                                        }
                                    }
                                } else if let Err(e) = result {
                                    // Notify failure
                                    if let Some(ref n) = notifier {
                                        n.notify_reclaim_failed(&pubkey.to_string(), &e.to_string()).await;
                                    }
                                }
                            }
                            info!("Saved {} reclaim operations to database", summary.successful);
                        }
                    }
                    
                    // Send batch summary notification
                    if let Some(ref n) = notifier {
                        let total_sol = solana::rent::RentCalculator::lamports_to_sol(summary.total_reclaimed);
                        n.notify_batch_complete(summary.successful, summary.failed, total_sol).await;
                    }
                    
                    // Print summary
                    summary.print_summary();
                }
                Err(e) => {
                    warn!("Batch processing failed: {}", e);
                    if let Some(ref n) = notifier {
                        n.notify_error(&format!("Batch processing failed: {}", e)).await;
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
    let _db = storage::Database::new(&config.database.path)?;
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