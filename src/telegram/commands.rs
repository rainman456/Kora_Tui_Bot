// src/telegram/commands.rs - Fixed with database persistence

use teloxide::prelude::*;
use teloxide::utils::command::BotCommands;
use std::sync::Arc;
use crate::telegram::bot::{BotState, Command};
use crate::kora::KoraMonitor;
use crate::reclaim::EligibilityChecker;
use crate::utils;
use crate::telegram::formatters::format_sol_tg;
use crate::storage::models::{SponsoredAccount, AccountStatus}; 
use tracing::{info, error}; 

/// Main command handler
pub async fn handle_command(
    bot: Bot, 
    msg: Message, 
    cmd: Command, 
    state: Arc<BotState>
) -> ResponseResult<()> {
    let user_id = msg.from().map(|u| u.id.0).unwrap_or(0);
    if let Some(telegram_config) = &state.config.telegram {
        if !telegram_config.authorized_users.is_empty() 
            && !telegram_config.authorized_users.contains(&user_id) {
            bot.send_message(msg.chat.id, "⛔ Authorization failed. You are not authorized to use this bot.")
                .await?;
            return Ok(());
        }
    }

    match cmd {
        Command::Start => handle_start(bot, msg).await,
        Command::Help => handle_help(bot, msg).await,
        Command::Status => handle_status(bot, msg, state).await,
        Command::Scan => handle_scan(bot, msg, state).await,
        Command::Accounts => handle_accounts(bot, msg, state).await,
        Command::Closed => handle_closed(bot, msg, state).await,
        Command::Reclaimed => handle_reclaimed(bot, msg, state).await,
        Command::Eligible => handle_eligible(bot, msg, state).await,
        Command::Stats => handle_stats(bot, msg, state).await,
        Command::Settings => handle_settings(bot, msg, state).await,
    }
}

async fn handle_start(bot: Bot, msg: Message) -> ResponseResult<()> {
    bot.send_message(
        msg.chat.id, 
        "👋 *Welcome to Kora Rent Reclaim Bot*\n\nI can help you monitor and reclaim rent from sponsored accounts\\.\n\nUse /help to see available commands\\.",
    )
    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
    .await?;
    Ok(())
}

async fn handle_help(bot: Bot, msg: Message) -> ResponseResult<()> {
    bot.send_message(msg.chat.id, Command::descriptions().to_string()).await?;
    Ok(())
}

async fn handle_status(bot: Bot, msg: Message, state: Arc<BotState>) -> ResponseResult<()> {
    let config = &state.config;
    let status_msg = format!(
        "🟢 *Bot Status: Online*\n\nNetwork: {}\nMode: {}\nDry Run: {}\nOperator: `{}`",
        match config.solana.network { 
            crate::config::Network::Mainnet => "Mainnet",
            crate::config::Network::Devnet => "Devnet",
            crate::config::Network::Testnet => "Testnet",
        },
        if config.reclaim.auto_reclaim_enabled { "Auto" } else { "Manual" },
        config.reclaim.dry_run,
        utils::format_pubkey(&config.kora.operator_pubkey)
    );
    bot.send_message(msg.chat.id, status_msg)
        .parse_mode(teloxide::types::ParseMode::MarkdownV2)
        .await?;
    Ok(())
}

// ✅ CRITICAL FIX: Persist scan results to database
async fn handle_scan(bot: Bot, msg: Message, state: Arc<BotState>) -> ResponseResult<()> {
    bot.send_message(msg.chat.id, "🔍 Scanning for sponsored accounts... This may take a moment.").await?;
    
    let operator_pubkey = match state.config.operator_pubkey() {
        Ok(pk) => pk,
        Err(e) => {
            bot.send_message(msg.chat.id, format!("❌ Error: {}", e)).await?;
            return Ok(());
        }
    };
    
    let monitor = KoraMonitor::new(state.rpc_client.clone(), operator_pubkey);
    
    match monitor.get_sponsored_accounts(100).await {
        Ok(accounts) => {
            let count = accounts.len();
            
            // ✅ FIX: Convert to database models and persist
            let db_accounts: Vec<SponsoredAccount> = accounts
                .iter()
                .map(|account_info| SponsoredAccount {
                    pubkey: account_info.pubkey.to_string(),
                    created_at: account_info.created_at,
                    closed_at: None,
                    rent_lamports: account_info.rent_lamports,
                    data_size: account_info.data_size,
                    status: AccountStatus::Active,
                    creation_signature: Some(account_info.creation_signature.to_string()),
                    creation_slot: Some(account_info.creation_slot),
                    close_authority: None,
                    reclaim_strategy: None,
                })
                .collect();
            
            // ✅ FIX: Save to database
            let db = state.database.lock().await;
            match db.save_accounts_batch(&db_accounts) {
                Ok(saved_count) => {
                    info!("Telegram /scan saved {} accounts to database", saved_count);
                    
                    // ✅ FIX: Update checkpoint
                    if let Some(latest_account) = accounts.first() {
                        let _ = db.save_last_processed_signature(
                            &latest_account.creation_signature.to_string()
                        );
                        let _ = db.save_last_processed_slot(latest_account.creation_slot);
                    }
                    
                    bot.send_message(
                        msg.chat.id,
                        format!(
                            "✅ Scan complete\\!\n\n\
                             Found: {} accounts\n\
                             Saved: {} to database\n\n\
                             Use /accounts to view them\\.",
                            count, saved_count
                        )
                    )
                    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                    .await?;
                }
                Err(e) => {
                    error!("Failed to save accounts from Telegram scan: {}", e);
                    bot.send_message(
                        msg.chat.id,
                        format!(
                            "⚠️ Found {} accounts but failed to save to database: {}\n\n\
                             Accounts were not persisted\\.",
                            count, e
                        )
                    )
                    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                    .await?;
                }
            }
        }
        Err(e) => {
            error!("Telegram /scan failed: {}", e);
            bot.send_message(msg.chat.id, format!("❌ Scan failed: {}", e)).await?;
        }
    }
    Ok(())
}

async fn handle_accounts(bot: Bot, msg: Message, state: Arc<BotState>) -> ResponseResult<()> {
    bot.send_message(msg.chat.id, "📋 Fetching account list...").await?;
    
    let db = state.database.lock().await;
    match db.get_active_accounts() {
        Ok(accounts) => {
            if accounts.is_empty() {
                bot.send_message(msg.chat.id, "No active accounts found in database. Run /scan first.").await?;
            } else {
                let count = accounts.len();
                let display_limit = std::cmp::min(count, 5);
                let mut response = format!("📋 *Active Accounts* ({})\\n\\n", count);
                
                for acc in &accounts[..display_limit] {
                    response.push_str(&format!("• `{}`\\n  Rent: {} lamports\\n\\n", acc.pubkey, acc.rent_lamports));
                }
                
                if count > display_limit {
                    response.push_str(&format!("_\\.\\.\\.and {} more_", count - display_limit));
                }
                
                bot.send_message(msg.chat.id, response)
                    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                    .await?;
            }
        }
        Err(e) => {
            bot.send_message(msg.chat.id, format!("❌ Database error: {}", e)).await?;
        }
    }
    Ok(())
}

async fn handle_closed(bot: Bot, msg: Message, state: Arc<BotState>) -> ResponseResult<()> {
    bot.send_message(msg.chat.id, "📋 Fetching closed accounts...").await?;
    
    let db = state.database.lock().await;
    match db.get_closed_accounts() {
        Ok(accounts) => {
            if accounts.is_empty() {
                bot.send_message(msg.chat.id, "No closed accounts found in database.").await?;
            } else {
                let count = accounts.len();
                let display_limit = std::cmp::min(count, 5);
                let mut response = format!("🔒 *Closed Accounts* ({})\\n\\n", count);
                
                for acc in &accounts[..display_limit] {
                    response.push_str(&format!("• `{}`\\n  Rent: {} lamports\\n\\n", acc.pubkey, acc.rent_lamports));
                }
                
                if count > display_limit {
                    response.push_str(&format!("_\\.\\.\\.and {} more_", count - display_limit));
                }
                
                bot.send_message(msg.chat.id, response)
                    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                    .await?;
            }
        }
        Err(e) => {
            bot.send_message(msg.chat.id, format!("❌ Database error: {}", e)).await?;
        }
    }
    Ok(())
}

async fn handle_reclaimed(bot: Bot, msg: Message, state: Arc<BotState>) -> ResponseResult<()> {
    bot.send_message(msg.chat.id, "📋 Fetching reclaimed accounts...").await?;
    
    let db = state.database.lock().await;
    match db.get_reclaimed_accounts() {
        Ok(accounts) => {
            if accounts.is_empty() {
                bot.send_message(msg.chat.id, "No reclaimed accounts found in database.").await?;
            } else {
                let count = accounts.len();
                let display_limit = std::cmp::min(count, 5);
                let mut response = format!("✅ *Reclaimed Accounts* ({})\\n\\n", count);
                
                for acc in &accounts[..display_limit] {
                    response.push_str(&format!("• `{}`\\n  Rent: {} lamports\\n\\n", acc.pubkey, acc.rent_lamports));
                }
                
                if count > display_limit {
                    response.push_str(&format!("_\\.\\.\\.and {} more_", count - display_limit));
                }
                
                bot.send_message(msg.chat.id, response)
                    .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                    .await?;
            }
        }
        Err(e) => {
            bot.send_message(msg.chat.id, format!("❌ Database error: {}", e)).await?;
        }
    }
    Ok(())
}

// ✅ FIX: Also persist eligible accounts check results
async fn handle_eligible(bot: Bot, msg: Message, state: Arc<BotState>) -> ResponseResult<()> {
    bot.send_message(msg.chat.id, "🔍 Checking eligibility...").await?;
    
    let operator_pubkey = match state.config.operator_pubkey() {
        Ok(pk) => pk,
        Err(e) => {
            bot.send_message(msg.chat.id, format!("❌ Error: {}", e)).await?;
            return Ok(());
        }
    };
    
    let monitor = KoraMonitor::new(state.rpc_client.clone(), operator_pubkey);
    
    match monitor.get_sponsored_accounts(50).await {
        Ok(accounts) => {
            let eligibility_checker = EligibilityChecker::new(state.rpc_client.clone(), state.config.clone());
            let mut eligible_count = 0;
            let mut total_reclaimable = 0u64;
            let mut eligible_accounts = Vec::new();
            
            for acc in &accounts {
                if let Ok(true) = eligibility_checker.is_eligible(&acc.pubkey, acc.created_at).await {
                    eligible_count += 1;
                    total_reclaimable += acc.rent_lamports;
                    eligible_accounts.push(acc.clone());
                }
            }
            
            // ✅ FIX: Save all scanned accounts to database (not just eligible ones)
            let db_accounts: Vec<SponsoredAccount> = accounts
                .iter()
                .map(|account_info| SponsoredAccount {
                    pubkey: account_info.pubkey.to_string(),
                    created_at: account_info.created_at,
                    closed_at: None,
                    rent_lamports: account_info.rent_lamports,
                    data_size: account_info.data_size,
                    status: AccountStatus::Active,
                    creation_signature: Some(account_info.creation_signature.to_string()),
                    creation_slot: Some(account_info.creation_slot),
                    close_authority: None,
                    reclaim_strategy: None,
                })
                .collect();
            
            let db = state.database.lock().await;
            if let Err(e) = db.save_accounts_batch(&db_accounts) {
                error!("Failed to save accounts from /eligible check: {}", e);
            }
            
            bot.send_message(
                msg.chat.id,
                format!(
                    "💰 *Eligibility Check*\\n\\nFound {} eligible accounts\\.\\nEst\\. reclaimable: {}", 
                    eligible_count,
                    format_sol_tg(total_reclaimable)
                )
            )
            .parse_mode(teloxide::types::ParseMode::MarkdownV2)
            .await?;
        }
        Err(e) => {
            error!("Telegram /eligible check failed: {}", e);
            bot.send_message(msg.chat.id, format!("❌ Error checking eligibility: {}", e)).await?;
        }
    }
    Ok(())
}

async fn handle_stats(bot: Bot, msg: Message, state: Arc<BotState>) -> ResponseResult<()> {
    let db = state.database.lock().await;
    match db.get_stats() {
        Ok(stats) => {
            let msg_text = format!(
                "📊 *Kora Bot Statistics*\\n\\n\
                *Accounts*\\n\
                Total: {}\\n\
                Active: {}\\n\
                Closed: {}\\n\
                Reclaimed: {}\\n\\n\
                *Operations*\\n\
                Total Ops: {}\\n\
                Reclaimed: {}\\n\
                Avg: {} lamports",
                stats.total_accounts,
                stats.active_accounts,
                stats.closed_accounts,
                stats.reclaimed_accounts,
                stats.total_operations,
                format_sol_tg(stats.total_reclaimed),
                stats.avg_reclaim_amount
            );
            bot.send_message(msg.chat.id, msg_text)
                .parse_mode(teloxide::types::ParseMode::MarkdownV2)
                .await?;
        }
        Err(e) => {
            bot.send_message(msg.chat.id, format!("❌ Error fetching stats: {}", e)).await?;
        }
    }
    Ok(())
}

async fn handle_settings(bot: Bot, msg: Message, state: Arc<BotState>) -> ResponseResult<()> {
    let config = &state.config;
    let settings_msg = format!(
        "⚙️ *Current Settings*\\n\\n\
        *RPC*: `{}`\\n\
        *Min Inactive*: {} days\\n\
        *Auto Reclaim*: {}\\n\
        *Batch Size*: {}\\n\
        *Dry Run*: {}\\n\
        *Database*: `{}`",
        config.solana.rpc_url,
        config.reclaim.min_inactive_days,
        if config.reclaim.auto_reclaim_enabled { "On" } else { "Off" },
        config.reclaim.batch_size,
        if config.reclaim.dry_run { "Yes" } else { "No" },
        config.database.path
    );
    bot.send_message(msg.chat.id, settings_msg)
        .parse_mode(teloxide::types::ParseMode::MarkdownV2)
        .await?;
    Ok(())
}