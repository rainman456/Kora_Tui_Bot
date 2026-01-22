use teloxide::{prelude::*, utils::command::BotCommands};
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::config::Config;
use crate::solana::SolanaRpcClient;
use crate::storage::Database;
use tracing::{info,  error};

/// State shared across all bot handlers
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
    #[command(description = "Show accounts eligible for reclaim")]
    Eligible,
    #[command(description = "Show statistics")]
    Stats,
    #[command(description = "View current settings")]
    Settings,
}

pub async fn run_telegram_bot(config: Config) -> crate::error::Result<()> {
    // Check if telegram config exists
    let telegram_config = if let Some(conf) = &config.telegram {
        conf
    } else {
        error!("Telegram configuration missing");
        return Err(crate::error::ReclaimError::Config("Telegram configuration missing".to_string()));
    };

    info!("Starting Telegram bot...");
    
    let bot = Bot::new(telegram_config.bot_token.clone());
    
    // Initialize clients
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
    
    // Wrap bot to add command handling
    let handler = dptree::entry()
        .branch(Update::filter_message()
            .filter_command::<Command>()
            .endpoint(crate::telegram::commands::answer))
        .branch(Update::filter_callback_query()
            .endpoint(crate::telegram::callbacks::handle_callback));

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![state])
        .enable_ctrlc_handler()
        .build()
        .dispatch()
        .await;

    Ok(())
}
