mod cli;
mod config;
mod error;
mod kora;
mod reclaim;
mod solana;
mod storage;
mod telegram;
mod treasury;
mod tui;
mod utils;

use clap::Parser;
use cli::{Cli, Commands};
use colored::*;
use config::Config;
use tracing::{debug, error, info, warn};

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
        Commands::Tui => run_tui(config).await,

        Commands::Scan {
            verbose,
            dry_run,
            limit,
        } => {
            info!("Scanning for eligible accounts...");
            scan_accounts(&config, verbose, dry_run, limit).await
        }

        Commands::Stats { format, total } => {
            info!("Generating statistics...");
            show_stats(&config, &format, total).await
        }

        Commands::PassiveCheck => {
            info!("Checking for passive reclaims...");
            check_passive_reclaims(&config).await
        }

        Commands::DailySummary => {
            info!("Sending daily summary...");
            send_daily_summary(&config).await
        }

        // ✅ NEW: List command using get_all_accounts
        Commands::List {
            status,
            format,
            detailed,
        } => {
            info!("Listing accounts with filter: {}", status);
            list_accounts(&config, &status, &format, detailed).await
        }

        // ✅ NEW: Reset command using clear_checkpoints
        Commands::Reset { yes } => {
            info!("Resetting checkpoints...");
            reset_checkpoints(&config, yes).await
        }

        // ✅ NEW: Checkpoints command using get_checkpoint_info
        Commands::Checkpoints => {
            info!("Showing checkpoint information...");
            show_checkpoints(&config).await
        }

        Commands::Reclaim {
            pubkey,
            yes,
            dry_run,
        } => {
            info!("Reclaiming account: {}", pubkey);
            reclaim_account(&config, &pubkey, yes, dry_run).await
        }

        Commands::Auto { interval, dry_run } => {
            info!(
                "Starting automated reclaim service (interval: {}s)",
                interval
            );
            run_auto_service(&config, interval, dry_run).await
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

async fn run_tui(config: Config) -> error::Result<()> {
    info!("Launching TUI...");
    tui::run_tui(config).await
}

async fn scan_accounts(
    config: &Config,
    verbose: bool,
    dry_run: bool,
    limit: Option<usize>,
) -> error::Result<()> {
    use solana_sdk::pubkey::Pubkey;

    println!("{}", "Scanning for eligible accounts...".cyan());

    let rpc_client = solana::SolanaRpcClient::new(
        &config.solana.rpc_url,
        config.commitment_config(),
        config.solana.rate_limit_delay_ms,
    );

    let operator_pubkey = config.operator_pubkey()?;
    let monitor = kora::KoraMonitor::new(rpc_client.clone(), operator_pubkey);

    let max_txns = limit.unwrap_or(5000);
    info!(
        "Discovering sponsored accounts from up to {} transactions",
        max_txns
    );

    let db = storage::Database::new(&config.database.path)?;

    // ✅ USE: get_all_accounts to cache existing accounts and avoid re-processing
    let existing_accounts = db.get_all_accounts()?;
    info!(
        "Found {} existing accounts in database",
        existing_accounts.len()
    );

    let existing_pubkeys: std::collections::HashSet<String> =
        existing_accounts.iter().map(|a| a.pubkey.clone()).collect();

    // ✅ USE: get_last_processed_slot to show scanning progress
    if let Ok(Some(last_slot)) = db.get_last_processed_slot() {
        println!(
            "Resuming from last checkpoint at slot: {}",
            last_slot.to_string().cyan()
        );
    }

    let sponsored_accounts = monitor.get_sponsored_accounts(max_txns).await?;

    // Calculate and log total locked rent
    if !sponsored_accounts.is_empty() {
        if let Ok(total_rent) = monitor.get_total_locked_rent(&sponsored_accounts).await {
            info!(
                "Total rent locked in sponsored accounts: {} SOL",
                utils::format_sol(total_rent)
            );
        }
    }

    println!("Found {} sponsored accounts", sponsored_accounts.len());

    // Separate new accounts from existing ones
    let mut new_accounts = Vec::new();
    let mut updated_accounts = 0;

    for account_info in &sponsored_accounts {
        let db_account = storage::models::SponsoredAccount {
            pubkey: account_info.pubkey.to_string(),
            created_at: account_info.created_at,
            closed_at: None,
            rent_lamports: account_info.rent_lamports,
            data_size: account_info.data_size,
            status: storage::models::AccountStatus::Active,
            creation_signature: Some(account_info.creation_signature.to_string()),
            creation_slot: Some(account_info.creation_slot),
            close_authority: None,
            reclaim_strategy: None,
        };

        if existing_pubkeys.contains(&account_info.pubkey.to_string()) {
            updated_accounts += 1;
        } else {
            new_accounts.push(account_info.clone());
        }

        // Save or update account
        let _ = db.save_account(&db_account);
    }

    info!(
        "Saved {} accounts to database ({} new, {} updated)",
        sponsored_accounts.len(),
        new_accounts.len(),
        updated_accounts
    );

    if !new_accounts.is_empty() {
        println!(
            "{} {} new accounts discovered",
            "✓".green(),
            new_accounts.len().to_string().cyan()
        );
    }

    let eligibility_checker = reclaim::EligibilityChecker::new(rpc_client.clone(), config.clone());

    let mut eligible_accounts = Vec::new();

    for account_info in &sponsored_accounts {
        // ✅ USE: is_account_active to check if account still exists before processing
        let is_active = match rpc_client.is_account_active(&account_info.pubkey).await {
            Ok(active) => active,
            Err(e) => {
                warn!(
                    "Failed to check if account {} is active: {}",
                    account_info.pubkey, e
                );
                // Assume inactive if check fails
                false
            }
        };

        if !is_active {
            debug!(
                "Account {} is no longer active, skipping eligibility check",
                account_info.pubkey
            );
            // Mark as closed in database
            let _ = db.update_account_status(
                &account_info.pubkey.to_string(),
                storage::models::AccountStatus::Closed,
            );
            continue;
        }

        // Skip already reclaimed accounts
        if let Some(existing) = existing_accounts
            .iter()
            .find(|a| a.pubkey == account_info.pubkey.to_string())
        {
            if existing.status == storage::models::AccountStatus::Reclaimed {
                continue;
            }
        }

        let is_eligible = eligibility_checker
            .is_eligible(&account_info.pubkey, account_info.created_at)
            .await?;

        if is_eligible {
            eligible_accounts.push(account_info.clone());
        }
    }

    let mut eligible = Vec::new();
    let mut total_reclaimable = 0u64;

    if !eligible_accounts.is_empty() {
        let pubkeys: Vec<Pubkey> = eligible_accounts.iter().map(|a| a.pubkey).collect();

        info!(
            "Fetching balances for {} eligible accounts in batch",
            pubkeys.len()
        );
        match rpc_client.get_multiple_accounts(&pubkeys).await {
            Ok(accounts) => {
                for (account_info, account_opt) in eligible_accounts.iter().zip(accounts.iter()) {
                    if let Some(account) = account_opt {
                        let balance = account.lamports;
                        total_reclaimable += balance;
                        eligible.push((account_info.clone(), balance));
                    }
                }
            }
            Err(e) => {
                warn!(
                    "Failed to batch fetch accounts, falling back to individual calls: {}",
                    e
                );
                for account_info in &eligible_accounts {
                    if let Ok(balance) = rpc_client.get_balance(&account_info.pubkey).await {
                        total_reclaimable += balance;
                        eligible.push((account_info.clone(), balance));
                    }
                }
            }
        }
    }

    // In scan_accounts(), after discovering accounts, add classification:

    println!("\n{}", "Analyzing reclaim strategies...".cyan());

    let eligibility_checker = reclaim::EligibilityChecker::new(rpc_client.clone(), config.clone());

    let mut active_count = 0;
    let mut passive_count = 0;
    let mut unrecoverable_count = 0;

    for account_info in &sponsored_accounts {
        // Determine strategy
        if let Ok((strategy, close_authority)) = eligibility_checker
            .determine_reclaim_strategy(&account_info.pubkey)
            .await
        {
            // Update database with strategy
            let _ = db.update_account_authority(
                &account_info.pubkey.to_string(),
                close_authority,
                &strategy.to_string(),
            );

            match strategy {
                storage::models::ReclaimStrategy::ActiveReclaim => active_count += 1,
                storage::models::ReclaimStrategy::PassiveMonitoring => passive_count += 1,
                storage::models::ReclaimStrategy::Unrecoverable => unrecoverable_count += 1,
                storage::models::ReclaimStrategy::Unknown => {}
            }
        }
    }

    println!("\n{}", "=== Reclaim Strategy Analysis ===".cyan().bold());
    println!(
        "Active Reclaim Possible:  {} accounts ✓",
        active_count.to_string().green()
    );
    println!(
        "Passive Monitoring:       {} accounts ⏱",
        passive_count.to_string().yellow()
    );
    println!(
        "Unrecoverable:            {} accounts ✗",
        unrecoverable_count.to_string().red()
    );

    // Display results
    println!("\n{}", "=== Scan Results ===".cyan().bold());
    println!("Total Sponsored:      {}", sponsored_accounts.len());
    println!(
        "Cached (existing):    {}",
        existing_accounts.len().to_string().yellow()
    );
    println!(
        "New accounts:         {}",
        new_accounts.len().to_string().green()
    );
    println!(
        "Eligible for Reclaim: {} ✓",
        eligible.len().to_string().green()
    );
    println!(
        "Total Reclaimable:    {}",
        utils::format_sol(total_reclaimable).cyan()
    );

    if verbose && !eligible.is_empty() {
        println!("\n{}", "Eligible Accounts:".yellow());
        utils::print_table_border(120);
        utils::print_table_row(
            &["Pubkey", "Balance", "Created", "Status", "Slot"],
            &[44, 20, 20, 15, 21],
        );
        utils::print_table_border(120);

        for (account, balance) in &eligible {
            // ✅ USE: get_account_creation_details for verbose output
            let slot_str = if let Ok(Some((_, creation_slot))) =
                db.get_account_creation_details(&account.pubkey.to_string())
            {
                creation_slot.to_string()
            } else {
                account.creation_slot.to_string()
            };

            utils::print_table_row(
                &[
                    &account.pubkey.to_string(),
                    &utils::format_sol(*balance),
                    &utils::format_timestamp(&account.created_at),
                    "Eligible",
                    &slot_str,
                ],
                &[44, 20, 20, 15, 21],
            );
        }
        utils::print_table_border(120);
    }

    if dry_run && !eligible.is_empty() {
        println!("\n{}", "DRY RUN: No transactions will be sent".yellow());
    }

    Ok(())
}

async fn reclaim_account(
    config: &Config,
    pubkey: &str,
    yes: bool,
    dry_run: bool,
) -> error::Result<()> {
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

    let db = storage::Database::new(&config.database.path)?;

    if let Ok(Some(db_account)) = db.get_account_by_pubkey(pubkey) {
        info!(
            "Account found in database with status: {:?}",
            db_account.status
        );
        println!("Account status in database: {:?}", db_account.status);

        // ✅ USE: get_account_creation_details to show when account was created
        if let Ok(Some((creation_sig, creation_slot))) = db.get_account_creation_details(pubkey) {
            println!(
                "Created at slot: {} (signature: {})",
                creation_slot.to_string().cyan(),
                utils::format_pubkey(&creation_sig)
            );

            // Calculate account age
            let account_age = chrono::Utc::now() - db_account.created_at;
            println!(
                "Account age: {} days",
                account_age.num_days().to_string().yellow()
            );
        }
    } else {
        info!("Account not found in database, proceeding with reclaim");
        println!("{}", "⚠️  Account not tracked in database".yellow());
    }

    // Verify sponsorship
    let operator_pubkey = config.operator_pubkey()?;
    let monitor = kora::KoraMonitor::new(rpc_client.clone(), operator_pubkey);

    info!(
        "Verifying if account {} is sponsored by Kora...",
        account_pubkey
    );
    if let Ok(is_sponsored) = monitor.is_kora_sponsored(&account_pubkey).await {
        if is_sponsored {
            info!("✓ Verified: Account is sponsored by Kora");
            println!("{}", "✓ Verified: Account is sponsored by Kora".green());
        } else {
            warn!("⚠️ Warning: Account does not appear to be sponsored by Kora operator");
            println!(
                "{}",
                "⚠️  Warning: Account not sponsored by Kora operator".yellow()
            );
            if !yes && !dry_run {
                if !utils::confirm_action("Account not sponsored by Kora. Continue anyway?") {
                    return Ok(());
                }
            }
        }
    }

    // Check eligibility
    let eligibility_checker = reclaim::EligibilityChecker::new(rpc_client.clone(), config.clone());

    // Get account info to determine creation time (use current time as fallback)
    let created_at = chrono::Utc::now() - chrono::Duration::days(365); // Assume old enough

    let reason = eligibility_checker
        .get_eligibility_reason(&account_pubkey, created_at)
        .await?;
    println!("Eligibility: {}", reason);

    let is_eligible = eligibility_checker
        .is_eligible(&account_pubkey, created_at)
        .await?;
    if !is_eligible {
        return Err(error::ReclaimError::NotEligible(reason));
    }

    // Get account balance
    let balance = rpc_client.get_balance(&account_pubkey).await?;
    println!("Account balance: {}", utils::format_sol(balance));

    // Confirm action
    if !yes && !dry_run {
        if !utils::confirm_action(&format!(
            "Reclaim {} from this account?",
            utils::format_sol(balance)
        )) {
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
    let account_type = kora::AccountType::SplToken;

    // Reclaim
    let result = engine
        .reclaim_account(&account_pubkey, &account_type)
        .await?;

    if let Some(sig) = result.signature {
        println!("✓ Reclaim successful!");
        println!("Account: {}", result.account);
        println!("Signature: {}", sig);
        println!("Reclaimed: {}", utils::format_sol(result.amount_reclaimed));

        // Save to database
        db.update_account_status(&pubkey, storage::models::AccountStatus::Reclaimed)?;

        db.save_reclaim_operation(&storage::models::ReclaimOperation {
            id: 0,
            account_pubkey: pubkey.to_string(),
            reclaimed_amount: result.amount_reclaimed,
            tx_signature: sig.to_string(),
            timestamp: chrono::Utc::now(),
            reason: "Manual CLI reclaim".to_string(),
        })?;

        info!("Reclaim operation saved to database");

        // Send notification if enabled
        if let Some(notifier) = telegram::AutoNotifier::new(config) {
            notifier
                .notify_reclaim_success(&pubkey, result.amount_reclaimed)
                .await;
        }
    } else if result.dry_run {
        println!(
            "DRY RUN: Would reclaim {}",
            utils::format_sol(result.amount_reclaimed)
        );
    }

    Ok(())
}



// Add this function to main.rs

async fn check_passive_reclaims(config: &Config) -> error::Result<()> {
    println!("{}", "Checking treasury for passive reclaims...".cyan());

    let rpc_client = solana::SolanaRpcClient::new(
        &config.solana.rpc_url,
        config.commitment_config(),
        config.solana.rate_limit_delay_ms,
    );

    let treasury_wallet = config.treasury_wallet()?;
    let db = storage::Database::new(&config.database.path)?;

    let monitor = treasury::TreasuryMonitor::new(treasury_wallet, rpc_client.clone(), db.clone());

    let passive_reclaims = monitor.check_for_passive_reclaims().await?;

    if passive_reclaims.is_empty() {
        println!("{}", "No passive reclaims detected".yellow());
        return Ok(());
    }

    println!("\n{} passive reclaim(s) detected:", passive_reclaims.len());

    for reclaim in &passive_reclaims {
        println!("\n{}", "═".repeat(80));
        println!("Amount: {}", utils::format_sol(reclaim.amount).green());
        println!("Confidence: {:?}", reclaim.confidence);
        println!("Timestamp: {}", utils::format_timestamp(&reclaim.timestamp));

        if !reclaim.attributed_accounts.is_empty() {
            println!("Likely from accounts:");
            for acc in &reclaim.attributed_accounts {
                println!("  • {}", acc);
            }
        }

        // Save to database
        let account_strs: Vec<String> = reclaim
            .attributed_accounts
            .iter()
            .map(|pk| pk.to_string())
            .collect();

        let confidence_str = format!("{:?}", reclaim.confidence);
        db.save_passive_reclaim(reclaim.amount, &account_strs, &confidence_str)?;
    }

    println!("\n{}", "═".repeat(80));

    let total_passive = monitor.get_total_passive_reclaimed()?;
    println!(
        "\nTotal passive reclaims recorded: {}",
        utils::format_sol(total_passive).green()
    );

    Ok(())
}

async fn run_auto_service(config: &Config, interval: u64, dry_run: bool) -> error::Result<()> {
    println!("{}", "Starting automated reclaim service...".green());

    let actual_interval = if interval > 0 {
        interval
    } else {
        config.reclaim.scan_interval_seconds
    };

    println!("Scan interval: {} seconds", actual_interval);
    println!("Dry run: {}", dry_run);

    let actual_dry_run = dry_run || config.reclaim.dry_run;
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
                    n.notify_error(&format!("Failed to get operator pubkey: {}", e))
                        .await;
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(actual_interval)).await;
                continue;
            }
        };

        let monitor = kora::KoraMonitor::new(rpc_client.clone(), operator_pubkey);

        // ✅ FIX: Use incremental scanning with checkpoints
        let db = match storage::Database::new(&config.database.path) {
            Ok(database) => database,
            Err(e) => {
                error!("Failed to open database: {}", e);
                if let Some(ref n) = notifier {
                    n.notify_error(&format!("Database error: {}", e)).await;
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(actual_interval)).await;
                continue;
            }
        };

        // ✅ Get last checkpoint signature for incremental scanning
        let since_signature = match db.get_last_processed_signature() {
            Ok(sig) => sig,
            Err(e) => {
                warn!("Failed to get checkpoint, doing full scan: {}", e);
                None
            }
        };

        // Discover new accounts (scan incrementally if checkpoint exists)
        let sponsored_accounts = match monitor.scan_new_accounts(since_signature, 5000).await {
            Ok(accounts) => accounts,
            Err(e) => {
                warn!("Failed to discover accounts: {}", e);
                if let Some(ref n) = notifier {
                    n.notify_error(&format!("Account discovery failed: {}", e))
                        .await;
                }
                tokio::time::sleep(tokio::time::Duration::from_secs(actual_interval)).await;
                continue;
            }
        };

        info!("Found {} sponsored accounts", sponsored_accounts.len());

        // ✅ Use batch save for efficiency
        if !sponsored_accounts.is_empty() {
            let db_accounts: Vec<storage::models::SponsoredAccount> = sponsored_accounts
                .iter()
                .map(|account_info| storage::models::SponsoredAccount {
                    pubkey: account_info.pubkey.to_string(),
                    created_at: account_info.created_at,
                    closed_at: None,
                    rent_lamports: account_info.rent_lamports,
                    data_size: account_info.data_size,
                    status: storage::models::AccountStatus::Active,
                    creation_signature: Some(account_info.creation_signature.to_string()),
                    creation_slot: Some(account_info.creation_slot),
                    close_authority: None,
                    reclaim_strategy: None,
                })
                .collect();

            match db.save_accounts_batch(&db_accounts) {
                Ok(saved) => info!("Batch saved {} accounts to database", saved),
                Err(e) => warn!("Failed to batch save accounts: {}", e),
            }

            // ✅ Update checkpoint with latest signature
            if let Some(latest_account) = sponsored_accounts.first() {
                let _ = db
                    .save_last_processed_signature(&latest_account.creation_signature.to_string());
                let _ = db.save_last_processed_slot(latest_account.creation_slot);
            }
        }

        // Check eligibility
        let eligibility_checker =
            reclaim::EligibilityChecker::new(rpc_client.clone(), config.clone());
        let mut eligible = Vec::new();

        for account_info in &sponsored_accounts {
            // ✅ Check if account already exists to avoid re-processing
            if let Ok(true) = db.account_exists(&account_info.pubkey.to_string()) {
                if let Ok(Some(db_account)) =
                    db.get_account_by_pubkey(&account_info.pubkey.to_string())
                {
                    // Skip already reclaimed accounts
                    if db_account.status == storage::models::AccountStatus::Reclaimed {
                        continue;
                    }
                }
            }

            if let Ok(true) = eligibility_checker
                .is_eligible(&account_info.pubkey, account_info.created_at)
                .await
            {
                eligible.push((account_info.pubkey, account_info.account_type.clone()));
            }
        }

        // Notify scan complete
        if let Some(ref n) = notifier {
            n.notify_scan_complete(sponsored_accounts.len(), eligible.len())
                .await;
        }

        if !eligible.is_empty() {
            info!("Found {} eligible accounts", eligible.len());

            // Load treasury and reclaim
            let treasury_keypair = match config.load_treasury_keypair() {
                Ok(kp) => kp,
                Err(e) => {
                    error!("Failed to load treasury keypair: {}", e);
                    if let Some(ref n) = notifier {
                        n.notify_error(&format!("Failed to load treasury keypair: {}", e))
                            .await;
                    }
                    tokio::time::sleep(tokio::time::Duration::from_secs(actual_interval)).await;
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

            // In run_auto_service(), add after the main reclaim logic:

            // Check for passive reclaims
            let treasury_wallet = config.treasury_wallet()?;
            let treasury_monitor =
                treasury::TreasuryMonitor::new(treasury_wallet, rpc_client.clone(), db.clone());

            match treasury_monitor.check_for_passive_reclaims().await {
                Ok(passive_reclaims) => {
                    if !passive_reclaims.is_empty() {
                        info!("Detected {} passive reclaim(s)", passive_reclaims.len());

                        for reclaim in &passive_reclaims {
                            let account_strs: Vec<String> = reclaim
                                .attributed_accounts
                                .iter()
                                .map(|pk| pk.to_string())
                                .collect();

                            let confidence_str = format!("{:?}", reclaim.confidence);
                            let _ = db.save_passive_reclaim(
                                reclaim.amount,
                                &account_strs,
                                &confidence_str,
                            );

                            // Notify
                            if let Some(ref n) = notifier {
                                n.notify_passive_reclaim(
                                    reclaim.amount,
                                    &account_strs,
                                    &confidence_str,
                                )
                                .await;
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to check for passive reclaims: {}", e);
                }
            }

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

                    if summary.successful > 0 {
                        for (pubkey, result) in &summary.results {
                            if let Ok(reclaim_result) = result {
                                if let Some(sig) = reclaim_result.signature {
                                    // Update account status
                                    let _ = db.update_account_status(
                                        &pubkey.to_string(),
                                        storage::models::AccountStatus::Reclaimed,
                                    );

                                    // Save reclaim operation
                                    let _ = db.save_reclaim_operation(
                                        &storage::models::ReclaimOperation {
                                            id: 0,
                                            account_pubkey: pubkey.to_string(),
                                            reclaimed_amount: reclaim_result.amount_reclaimed,
                                            tx_signature: sig.to_string(),
                                            timestamp: chrono::Utc::now(),
                                            reason: "Automated batch reclaim".to_string(),
                                        },
                                    );

                                    // Send individual success notification for high-value reclaims
                                    if let Some(ref n) = notifier {
                                        if let Some(tg_config) = &config.telegram {
                                            n.notify_high_value_reclaim(
                                                &pubkey.to_string(),
                                                reclaim_result.amount_reclaimed,
                                                tg_config.alert_threshold_sol,
                                            )
                                            .await;
                                        }
                                    }
                                }
                            } else if let Err(e) = result {
                                // Notify failure
                                if let Some(ref n) = notifier {
                                    n.notify_reclaim_failed(&pubkey.to_string(), &e.to_string())
                                        .await;
                                }
                            }
                        }
                        info!(
                            "Saved {} reclaim operations to database",
                            summary.successful
                        );
                    }

                    // Send batch summary notification
                    if let Some(ref n) = notifier {
                        let total_sol =
                            solana::rent::RentCalculator::lamports_to_sol(summary.total_reclaimed);
                        n.notify_batch_complete(summary.successful, summary.failed, total_sol)
                            .await;
                    }

                    // Print summary
                    summary.print_summary();
                }
                Err(e) => {
                    warn!("Batch processing failed: {}", e);
                    if let Some(ref n) = notifier {
                        n.notify_error(&format!("Batch processing failed: {}", e))
                            .await;
                    }
                }
            }
        } else {
            info!("No eligible accounts found");
        }

        tokio::time::sleep(tokio::time::Duration::from_secs(actual_interval)).await;
    }
}
async fn show_stats(config: &Config, format: &str, total_only: bool) -> error::Result<()> {
    let db = storage::Database::new(&config.database.path)?;

    // ✅ USE: get_total_reclaimed for lightweight query
    if total_only {
        let total = db.get_total_reclaimed()?;
        if format == "json" {
            println!(
                "{}",
                serde_json::json!({
                    "total_reclaimed": total,
                    "total_reclaimed_sol": utils::format_sol(total)
                })
            );
        } else {
            println!(
                "Total Reclaimed: {}",
                utils::format_sol(total).green().bold()
            );
        }
        return Ok(());
    }

    let stats = db.get_stats()?;

    if format == "json" {
        // JSON output with passive reclaims
        let checkpoints = db.get_checkpoint_info().unwrap_or_default();
        let checkpoint_map: std::collections::HashMap<String, String> = checkpoints
            .into_iter()
            .map(|(key, value, _)| (key, value))
            .collect();

        let passive_total = db.get_total_passive_reclaimed().unwrap_or(0);

        let active_accounts = db
            .get_accounts_by_strategy("ActiveReclaim")
            .unwrap_or_default();
        let passive_accounts = db
            .get_accounts_by_strategy("PassiveMonitoring")
            .unwrap_or_default();
        let unrecoverable = db
            .get_accounts_by_strategy("Unrecoverable")
            .unwrap_or_default();

        let active_rent: u64 = active_accounts.iter().map(|a| a.rent_lamports).sum();
        let passive_rent: u64 = passive_accounts.iter().map(|a| a.rent_lamports).sum();
        let unrecoverable_rent: u64 = unrecoverable.iter().map(|a| a.rent_lamports).sum();

        let json_output = serde_json::json!({
            "stats": stats,
            "checkpoints": checkpoint_map,
            "passive_reclaims": {
                "total_amount": passive_total,
                "total_amount_sol": crate::solana::rent::RentCalculator::lamports_to_sol(passive_total),
            },
            "reclaim_strategies": {
                "active_reclaim": {
                    "accounts": active_accounts.len(),
                    "total_rent": active_rent,
                    "total_rent_sol": crate::solana::rent::RentCalculator::lamports_to_sol(active_rent),
                },
                "passive_monitoring": {
                    "accounts": passive_accounts.len(),
                    "total_rent": passive_rent,
                    "total_rent_sol": crate::solana::rent::RentCalculator::lamports_to_sol(passive_rent),
                },
                "unrecoverable": {
                    "accounts": unrecoverable.len(),
                    "total_rent": unrecoverable_rent,
                    "total_rent_sol": crate::solana::rent::RentCalculator::lamports_to_sol(unrecoverable_rent),
                },
            }
        });

        println!("{}", serde_json::to_string_pretty(&json_output)?);
        return Ok(());
    }

    // Enhanced table format
    println!("{}", "=== Kora Rent Reclaim Statistics ===".cyan().bold());

    println!("\n{}", "Accounts:".cyan());
    println!("  Total:      {}", stats.total_accounts);
    println!(
        "  Active:     {}",
        stats.active_accounts.to_string().green()
    );
    println!(
        "  Closed:     {}",
        stats.closed_accounts.to_string().yellow()
    );
    println!(
        "  Reclaimed:  {}",
        stats.reclaimed_accounts.to_string().cyan()
    );

    // NEW: Reclaim strategy breakdown
    println!("\n{}", "Reclaim Strategy Analysis:".cyan().bold());

    let active_accounts = db
        .get_accounts_by_strategy("ActiveReclaim")
        .unwrap_or_default();
    let passive_accounts = db
        .get_accounts_by_strategy("PassiveMonitoring")
        .unwrap_or_default();
    let unrecoverable = db
        .get_accounts_by_strategy("Unrecoverable")
        .unwrap_or_default();

    let active_rent: u64 = active_accounts
        .iter()
        .filter(|a| a.status == storage::models::AccountStatus::Active)
        .map(|a| a.rent_lamports)
        .sum();
    let passive_rent: u64 = passive_accounts
        .iter()
        .filter(|a| a.status == storage::models::AccountStatus::Active)
        .map(|a| a.rent_lamports)
        .sum();
    let unrecoverable_rent: u64 = unrecoverable
        .iter()
        .filter(|a| a.status == storage::models::AccountStatus::Active)
        .map(|a| a.rent_lamports)
        .sum();

    println!("  {} Active Reclaim Possible:", "✓".green());
    println!(
        "    {} accounts | {} locked",
        active_accounts.len().to_string().green(),
        utils::format_sol(active_rent).green()
    );
    println!("    → Operator has close authority, can reclaim anytime");

    println!("\n  {} Passive Monitoring:", "⏱".yellow());
    println!(
        "    {} accounts | {} locked",
        passive_accounts.len().to_string().yellow(),
        utils::format_sol(passive_rent).yellow()
    );
    println!("    → User controls account, monitor for when they close it");

    println!("\n  {} Unrecoverable:", "✗".red());
    println!(
        "    {} accounts | {} locked",
        unrecoverable.len().to_string().red(),
        utils::format_sol(unrecoverable_rent).red()
    );
    println!("    → System accounts or permanently locked");

    // Reclaim operations
    println!("\n{}", "Reclaim Operations:".cyan());
    println!("  Active Reclaims:   {}", stats.total_operations);
    println!(
        "  Total SOL:         {}",
        utils::format_sol(stats.total_reclaimed)
    );
    println!(
        "  Average:           {}",
        utils::format_sol(stats.avg_reclaim_amount)
    );

    // NEW: Passive reclaims
    let passive_total = db.get_total_passive_reclaimed().unwrap_or(0);
    if passive_total > 0 {
        println!(
            "\n  Passive Reclaims:  {}",
            utils::format_sol(passive_total).green()
        );
        println!("  (Rent that returned to treasury when users closed accounts)");
    }

    // Total recovery
    let total_recovered = stats.total_reclaimed + passive_total;
    if total_recovered > 0 {
        println!(
            "\n  {} Total Recovered:  {}",
            "💰".green(),
            utils::format_sol(total_recovered).green().bold()
        );
    }

    // Scanning Progress
    println!("\n{}", "Scanning Progress:".cyan());
    match db.get_checkpoint_info() {
        Ok(checkpoints) => {
            if checkpoints.is_empty() {
                println!("  No checkpoints found (full scan on next run)");
            } else {
                for (key, value, updated_at) in checkpoints {
                    if key == "treasury_balance" {
                        let balance = value.parse::<u64>().unwrap_or(0);
                        println!(
                            "  Treasury Balance: {} (last checked: {})",
                            utils::format_sol(balance),
                            updated_at
                        );
                        continue;
                    }

                    let display_value = if key == "last_signature" {
                        utils::format_pubkey(&value)
                    } else {
                        value
                    };

                    let time_display =
                        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&updated_at) {
                            utils::format_timestamp(&dt.with_timezone(&chrono::Utc))
                        } else {
                            updated_at
                        };

                    println!(
                        "  {}: {} (updated: {})",
                        key.replace('_', " ").to_uppercase(),
                        display_value,
                        time_display
                    );
                }
            }
        }
        Err(e) => {
            warn!("Failed to get checkpoint info: {}", e);
            println!("  Error reading checkpoints: {}", e);
        }
    }

    // Show passive reclaim history if available
    let passive_history = db.get_passive_reclaim_history(Some(5)).unwrap_or_default();
    if !passive_history.is_empty() {
        println!("\n{}", "Recent Passive Reclaims:".yellow());
        utils::print_table_border(100);
        utils::print_table_row(
            &["Timestamp", "Amount", "Confidence", "Accounts"],
            &[22, 18, 15, 45],
        );
        utils::print_table_border(100);

        for record in passive_history {
            let accounts_str = if record.attributed_accounts.len() <= 2 {
                record
                    .attributed_accounts
                    .iter()
                    .map(|a| utils::format_pubkey(a))
                    .collect::<Vec<_>>()
                    .join(", ")
            } else {
                format!("{} accounts", record.attributed_accounts.len())
            };

            utils::print_table_row(
                &[
                    &utils::format_timestamp(&record.timestamp),
                    &utils::format_sol(record.amount),
                    &record.confidence,
                    &accounts_str,
                ],
                &[22, 18, 15, 45],
            );
        }
        utils::print_table_border(100);
    }

    // Show recent active reclaim history
    let history = db.get_reclaim_history(Some(10))?;
    if !history.is_empty() {
        println!("\n{}", "Recent Active Reclaim Operations:".yellow());
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

    // Recommendations
    println!("\n{}", "💡 Recommendations:".yellow().bold());
    if passive_accounts.len() > 0 {
        println!(
            "  • {} accounts with user authority may return rent when closed",
            passive_accounts.len()
        );
        println!(
            "    Run {} to check for passive reclaims",
            "kora-reclaim passive-check".cyan()
        );
    }
    if active_accounts.len() > 0 {
        println!(
            "  • {} accounts are eligible for active reclaim",
            active_accounts.len()
        );
        println!(
            "    Run {} to reclaim now",
            "kora-reclaim auto --dry-run".cyan()
        );
    }
    if unrecoverable.len() > 0 {
        println!(
            "  • {} accounts have permanently locked rent",
            unrecoverable.len()
        );
        println!("    Consider negotiating close authority with integrated apps");
    }

    Ok(())
}

async fn list_accounts(
    config: &Config,
    status_filter: &str,
    format: &str,
    detailed: bool,
) -> error::Result<()> {
    let db = storage::Database::new(&config.database.path)?;

    // ✅ USE: get_all_accounts to list everything
    let all_accounts = db.get_all_accounts()?;

    let filtered_accounts: Vec<_> = match status_filter.to_lowercase().as_str() {
        "active" => all_accounts
            .into_iter()
            .filter(|a| a.status == storage::models::AccountStatus::Active)
            .collect(),
        "closed" => all_accounts
            .into_iter()
            .filter(|a| a.status == storage::models::AccountStatus::Closed)
            .collect(),
        "reclaimed" => all_accounts
            .into_iter()
            .filter(|a| a.status == storage::models::AccountStatus::Reclaimed)
            .collect(),
        "all" => all_accounts,
        _ => {
            println!(
                "{}",
                "Invalid status filter. Use: active, closed, reclaimed, or all".red()
            );
            return Ok(());
        }
    };

    if format == "json" {
        // JSON output
        let json_data: Vec<serde_json::Value> = filtered_accounts
            .iter()
            .map(|acc| {
                let mut obj = serde_json::json!({
                    "pubkey": acc.pubkey,
                    "created_at": acc.created_at.to_rfc3339(),
                    "rent_lamports": acc.rent_lamports,
                    "data_size": acc.data_size,
                    "status": format!("{:?}", acc.status),
                });

                if detailed {
                    // ✅ USE: get_account_creation_details for detailed view
                    if let Ok(Some((creation_sig, creation_slot))) =
                        db.get_account_creation_details(&acc.pubkey)
                    {
                        obj["creation_signature"] = serde_json::json!(creation_sig);
                        obj["creation_slot"] = serde_json::json!(creation_slot);
                    }
                }

                obj
            })
            .collect();

        println!("{}", serde_json::to_string_pretty(&json_data)?);
        return Ok(());
    }

    // Table output
    println!(
        "{}",
        format!("=== Tracked Accounts ({}) ===", filtered_accounts.len())
            .cyan()
            .bold()
    );

    if filtered_accounts.is_empty() {
        println!("No accounts found matching filter: {}", status_filter);
        return Ok(());
    }

    if detailed {
        utils::print_table_border(120);
        utils::print_table_row(
            &[
                "Pubkey",
                "Status",
                "Created",
                "Balance",
                "Slot",
                "Signature",
            ],
            &[44, 10, 20, 15, 10, 21],
        );
        utils::print_table_border(120);

        for acc in &filtered_accounts {
            // ✅ USE: get_account_creation_details for each account
            let (slot_str, sig_str) = if let Ok(Some((creation_sig, creation_slot))) =
                db.get_account_creation_details(&acc.pubkey)
            {
                (
                    creation_slot.to_string(),
                    utils::format_pubkey(&creation_sig),
                )
            } else {
                ("N/A".to_string(), "N/A".to_string())
            };

            utils::print_table_row(
                &[
                    &utils::format_pubkey(&acc.pubkey),
                    &format!("{:?}", acc.status),
                    &utils::format_timestamp(&acc.created_at),
                    &utils::format_sol(acc.rent_lamports),
                    &slot_str,
                    &sig_str,
                ],
                &[44, 10, 20, 15, 10, 21],
            );
        }
        utils::print_table_border(120);
    } else {
        utils::print_table_border(90);
        utils::print_table_row(
            &["Pubkey", "Status", "Created", "Balance"],
            &[44, 12, 20, 14],
        );
        utils::print_table_border(90);

        for acc in &filtered_accounts {
            utils::print_table_row(
                &[
                    &utils::format_pubkey(&acc.pubkey),
                    &format!("{:?}", acc.status),
                    &utils::format_timestamp(&acc.created_at),
                    &utils::format_sol(acc.rent_lamports),
                ],
                &[44, 12, 20, 14],
            );
        }
        utils::print_table_border(90);
    }

    println!(
        "\nTotal: {} accounts | Active: {} | Closed: {} | Reclaimed: {}",
        filtered_accounts.len(),
        filtered_accounts
            .iter()
            .filter(|a| a.status == storage::models::AccountStatus::Active)
            .count(),
        filtered_accounts
            .iter()
            .filter(|a| a.status == storage::models::AccountStatus::Closed)
            .count(),
        filtered_accounts
            .iter()
            .filter(|a| a.status == storage::models::AccountStatus::Reclaimed)
            .count(),
    );

    Ok(())
}

async fn reset_checkpoints(config: &Config, yes: bool) -> error::Result<()> {
    println!("{}", "Resetting scanning checkpoints...".yellow());

    let db = storage::Database::new(&config.database.path)?;

    // ✅ USE: get_checkpoint_info to show what will be cleared
    match db.get_checkpoint_info() {
        Ok(checkpoints) => {
            if checkpoints.is_empty() {
                println!("No checkpoints to clear.");
                return Ok(());
            }

            println!("\nCurrent checkpoints:");
            for (key, value, updated_at) in &checkpoints {
                println!("  {} = {} (updated: {})", key, value, updated_at);
            }

            if !yes {
                println!(
                    "\n{}",
                    "⚠️  WARNING: This will force a full rescan on the next run!"
                        .yellow()
                        .bold()
                );
                if !utils::confirm_action("Are you sure you want to reset all checkpoints?") {
                    println!("Cancelled");
                    return Ok(());
                }
            }

            // ✅ USE: clear_checkpoints
            db.clear_checkpoints()?;
            println!("{}", "✓ All checkpoints cleared successfully".green());
            println!("The next scan will be a full scan from the beginning.");
        }
        Err(e) => {
            println!("Error reading checkpoints: {}", e);
        }
    }

    Ok(())
}

async fn show_checkpoints(config: &Config) -> error::Result<()> {
    let db = storage::Database::new(&config.database.path)?;

    println!("{}", "=== Scanning Checkpoints ===".cyan().bold());

    match db.get_checkpoint_info() {
        Ok(checkpoints) => {
            if checkpoints.is_empty() {
                println!("\nNo checkpoints found.");
                println!(
                    "Run {} to start tracking scan progress.",
                    "kora-reclaim scan".yellow()
                );
                return Ok(());
            }

            println!("\n{}", "Active Checkpoints:".cyan());
            utils::print_table_border(90);
            utils::print_table_row(&["Key", "Value", "Last Updated"], &[20, 44, 26]);
            utils::print_table_border(90);

            for (key, value, updated_at) in checkpoints {
                let display_value = if key == "last_signature" {
                    utils::format_pubkey(&value)
                } else {
                    value
                };

                let time_display = if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&updated_at)
                {
                    utils::format_timestamp(&dt.with_timezone(&chrono::Utc))
                } else {
                    updated_at
                };

                utils::print_table_row(
                    &[
                        &key.replace('_', " ").to_uppercase(),
                        &display_value,
                        &time_display,
                    ],
                    &[20, 44, 26],
                );
            }
            utils::print_table_border(90);
        }
        Err(e) => {
            println!("Error reading checkpoints: {}", e);
        }
    }

    println!("\n{}", "Scanning Progress:".cyan());
    if let Ok(Some(last_slot)) = db.get_last_processed_slot() {
        println!("  Last Processed Slot: {}", last_slot.to_string().cyan());

        // ✅ FIX: Actually use the rpc_client
        let rpc_client = solana::SolanaRpcClient::new(
            &config.solana.rpc_url,
            config.commitment_config(),
            config.solana.rate_limit_delay_ms,
        );

        // Get current slot to compare
        match rpc_client.client.get_slot() {
            Ok(current_slot) => {
                let slots_behind = current_slot.saturating_sub(last_slot);
                println!(
                    "  Current Network Slot: {}",
                    current_slot.to_string().cyan()
                );

                if slots_behind > 0 {
                    println!("  Slots Behind: {}", slots_behind.to_string().yellow());
                    // Roughly 400ms per slot on Solana mainnet
                    let minutes_behind = (slots_behind as f64 * 0.4) / 60.0;
                    if minutes_behind >= 1.0 {
                        println!("  Est. Time Behind: ~{:.1} minutes", minutes_behind);
                    }
                } else {
                    println!("  Status: Up to date ✓");
                }
            }
            Err(e) => {
                warn!("Could not fetch current slot: {}", e);
            }
        }

        println!("  Status: Incremental scanning enabled");
    } else {
        println!("  No slot checkpoint found");
        println!("  Status: Full scan mode");
    }

    println!(
        "\nTip: Use {} to reset checkpoints and force a full rescan",
        "kora-reclaim reset".yellow()
    );

    Ok(())
}

// Update the initialize function to use checkpoint info
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
    println!(
        "  Min Inactive:   {} days",
        config.reclaim.min_inactive_days
    );

    // ✅ USE: get_checkpoint_info in init to show scanning state
    println!("\n{}", "Scanning State:".cyan());
    match db.get_checkpoint_info() {
        Ok(checkpoints) => {
            if checkpoints.is_empty() {
                println!("  No checkpoints found (will perform full scan)");
            } else {
                println!("  Checkpoints found: {}", checkpoints.len());
                for (key, value, _) in checkpoints {
                    let display_value = if key == "last_signature" {
                        utils::format_pubkey(&value)
                    } else {
                        value
                    };
                    println!("    {}: {}", key, display_value);
                }
            }
        }
        Err(e) => {
            println!("  Error reading checkpoints: {}", e);
        }
    }

    println!("\n{}", "Ready to use! Try running:".cyan());
    println!(
        "  {} to scan for eligible accounts",
        "kora-reclaim scan --verbose".yellow()
    );
    println!(
        "  {} to list all tracked accounts",
        "kora-reclaim list --detailed".yellow()
    );
    println!(
        "  {} to view checkpoint status",
        "kora-reclaim checkpoints".yellow()
    );
    println!("  {} to view statistics", "kora-reclaim stats".yellow());
    println!("  {} to launch TUI dashboard", "kora-reclaim tui".yellow());
    Ok(())
}

async fn send_daily_summary(config: &Config) -> error::Result<()> {
    println!("{}", "Generating daily summary...".cyan());

    let db = storage::Database::new(&config.database.path)?;

    // Get operations from last 24 hours
    let all_ops = db.get_reclaim_history(None)?;
    let now = chrono::Utc::now();
    let yesterday = now - chrono::Duration::hours(24);

    let daily_ops: Vec<_> = all_ops
        .into_iter()
        .filter(|op| op.timestamp > yesterday)
        .collect();

    let total_reclaimed: u64 = daily_ops.iter().map(|op| op.reclaimed_amount).sum();

    let operations_count = daily_ops.len();

    println!("Operations in last 24h: {}", operations_count);
    println!("Total reclaimed: {}", utils::format_sol(total_reclaimed));

    // ✅ USE: notify_daily_summary
    if let Some(notifier) = telegram::AutoNotifier::new(config) {
        notifier
            .notify_daily_summary(total_reclaimed, operations_count)
            .await;
        println!("{}", "✓ Daily summary sent via Telegram".green());
    } else {
        println!("{}", "⚠️  Telegram not configured".yellow());
    }

    Ok(())
}
