use teloxide::prelude::*;
use crate::config::Config;
use tracing::error;

pub struct NotificationSystem {
    bot: Bot,
    config: Config,
}

impl NotificationSystem {
    pub fn new(bot_token: String, config: Config) -> Self {
        let bot = Bot::new(bot_token);
        Self { bot, config }
    }

    /// Send alert to all authorized users
    pub async fn send_alert(&self, message: &str) {
        if let Some(telegram_config) = &self.config.telegram {
            if !telegram_config.notifications_enabled {
                return;
            }

            for user_id in &telegram_config.authorized_users {
                let chat_id = ChatId(*user_id as i64);
                
                if let Err(e) = self.bot.send_message(chat_id, message).await {
                    error!("Failed to send notification to user {}: {}", user_id, e);
                }
            }
        }
    }
    
    /// Send alert only if amount exceeds threshold
    pub async fn send_reclaim_alert(&self, amount_sol: f64, message: &str) {
         if let Some(telegram_config) = &self.config.telegram {
             if amount_sol >= telegram_config.alert_threshold_sol {
                 self.send_alert(message).await;
             }
         }
    }
}
