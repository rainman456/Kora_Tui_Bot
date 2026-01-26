// src/telegram/bot.rs - Complete Rewrite

use teloxide::{prelude::*, utils::command::BotCommands};
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::config::Config;
use crate::solana::SolanaRpcClient;
use crate::storage::Database;
use tracing::{info, error};

/// State shared across all bot handlers
#[derive(Clone)]
pub struct BotState {
    pub config: Config,
    pub rpc_client: SolanaRpcClient,
    pub database: Arc<Mutex<Database>>,
}

#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "These commands are supported:")]
pub enum Command {
    #[command(description = "Start interaction with the bot")]
    Start,
    #[command(description = "Show help message")]
    Help,
    #[command(description = "Show bot status")]
    Status,
    #[command(description = "Scan for sponsored accounts")]
    Scan,
    #[command(description = "List recent sponsored accounts")]
    Accounts,
    #[command(description = "Show closed accounts")]
    Closed,
    #[command(description = "Show reclaimed accounts")]
    Reclaimed,
    #[command(description = "Show accounts eligible for reclaim")]
    Eligible,
    #[command(description = "Show statistics")]
    Stats,
    #[command(description = "View current settings")]
    Settings,
}

pub async fn run_telegram_bot(config: Config) -> crate::error::Result<()> {
    let telegram_config = if let Some(conf) = &config.telegram {
        conf
    } else {
        error!("Telegram configuration missing");
        return Err(crate::error::ReclaimError::Config("Telegram configuration missing".to_string()));
    };

    info!("Starting Telegram bot...");
    
    let bot = Bot::new(telegram_config.bot_token.clone());
    
    let rpc_client = SolanaRpcClient::new(
        &config.solana.rpc_url,
        config.commitment_config(),
        config.solana.rate_limit_delay_ms,
    );
    
    let database = Arc::new(Mutex::new(Database::new(&config.database.path)?));
    
    let state = Arc::new(BotState {
        config: config.clone(),
        rpc_client,
        database,
    });

    // Message handler for commands
    let command_handler = Update::filter_message()
        .branch(
            dptree::entry()
                .filter_command::<Command>()
                .endpoint({
                    let state = Arc::clone(&state);
                    move |bot: Bot, msg: Message, cmd: Command| {
                        let state = Arc::clone(&state);
                        async move {
                            crate::telegram::commands::handle_command(bot, msg, cmd, state).await
                        }
                    }
                })
        );
    
    // ✅ USE: Callback handler for inline button responses
    let callback_handler = Update::filter_callback_query()
        .endpoint({
            let state = Arc::clone(&state);
            move |bot: Bot, q: CallbackQuery| {
                let state = Arc::clone(&state);
                async move {
                    crate::telegram::callbacks::handle_callback(bot, q, state).await
                }
            }
        });

    // Combine both handlers
    let handler = dptree::entry()
        .branch(command_handler)
        .branch(callback_handler);

    Dispatcher::builder(bot, handler)
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}