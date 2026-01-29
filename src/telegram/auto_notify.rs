// src/telegram/auto_notify.rs - COMPLETE FIXED VERSION

use teloxide::prelude::*;
use teloxide::types::{ChatId, ParseMode};
use tracing::{info, error};
use crate::config::Config;

pub struct AutoNotifier {
    bot: Bot,
    chat_ids: Vec<i64>,
    enabled: bool,
}

impl AutoNotifier {
    pub fn new(config: &Config) -> Option<Self> {
        if let Some(telegram_config) = &config.telegram {
            if !telegram_config.notifications_enabled {
                info!("Telegram notifications are disabled in config");
                return None;
            }

            if telegram_config.authorized_users.is_empty() {
                info!("No authorized users configured for notifications");
                return None;
            }

            let bot = Bot::new(telegram_config.bot_token.clone());
            let chat_ids: Vec<i64> = telegram_config.authorized_users
                .iter()
                .map(|&id| id as i64)
                .collect();

            info!("Auto-notifier initialized for {} users", chat_ids.len());

            Some(Self {
                bot,
                chat_ids,
                enabled: true,
            })
        } else {
            None
        }
    }

    /// Send message to all authorized users
    async fn send_message(&self, message: &str) {
        if !self.enabled {
            return;
        }

        for chat_id in &self.chat_ids {
            match self.bot
                .send_message(ChatId(*chat_id), message)
                .parse_mode(ParseMode::MarkdownV2)
                .await
            {
                Ok(_) => {
                    info!("Notification sent to chat {}", chat_id);
                }
                Err(e) => {
                    error!("Failed to send Telegram message to {}: {}", chat_id, e);
                }
            }
        }
    }

    /// Send passive reclaim notification
    pub async fn notify_passive_reclaim(
        &self,
        amount: u64,
        accounts: &[String],
        confidence: &str,
    ) {
        if !self.enabled {
            return;
        }
        
        let sol_amount = crate::solana::rent::RentCalculator::lamports_to_sol(amount);
        
        let accounts_str = if accounts.len() <= 3 {
            accounts.iter()
                .map(|a| {
                    // Format pubkey for display
                    let short = if a.len() > 12 {
                        format!("{}...{}", &a[..6], &a[a.len()-6..])
                    } else {
                        a.clone()
                    };
                    format!("• `{}`", short)
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            format!("{} accounts", accounts.len())
        };
        
        let message = format!(
            "🔄 *Passive Reclaim Detected*\n\n\
             Amount: *{:.9} SOL*\n\
             Confidence: {}\n\
             Likely from:\n{}\n\n\
             This rent returned to treasury when the user closed their account.",
            sol_amount,
            confidence,
            accounts_str
        );
        
        self.send_message(&message).await;
    }

    /// Send scan complete notification
    pub async fn notify_scan_complete(&self, total: usize, eligible: usize) {
        if !self.enabled {
            return;
        }

        let message = format!(
            "🔍 *Scan Complete*\n\n\
            📊 Total sponsored accounts: {}\n\
            ✅ Eligible for reclaim: {}\n\n\
            _Automated scan completed successfully_",
            total, eligible
        );

        self.send_message(&message).await;
    }

    /// Send reclaim success notification
    pub async fn notify_reclaim_success(&self, pubkey: &str, amount: u64) {
        if !self.enabled {
            return;
        }

        let sol_amount = crate::solana::rent::RentCalculator::lamports_to_sol(amount);
        let message = format!(
            "✅ *Reclaim Successful*\n\n\
            Account: `{}`\n\
            Amount: *{:.9} SOL*\n\n\
            _Rent successfully reclaimed to treasury_",
            Self::format_pubkey(pubkey),
            sol_amount
        );

        self.send_message(&message).await;
    }

    /// Send reclaim failure notification
    pub async fn notify_reclaim_failed(&self, pubkey: &str, error: &str) {
        if !self.enabled {
            return;
        }

        let message = format!(
            "❌ *Reclaim Failed*\n\n\
            Account: `{}`\n\
            Error: {}\n\n\
            _Check logs for more details_",
            Self::format_pubkey(pubkey),
            error
        );

        self.send_message(&message).await;
    }

    /// Send batch complete notification
    pub async fn notify_batch_complete(&self, successful: usize, failed: usize, total_sol: f64) {
        if !self.enabled {
            return;
        }

        let emoji = if failed == 0 { "🎉" } else { "📦" };
        let message = format!(
            "{} *Batch Reclaim Complete*\n\n\
            ✅ Successful: {}\n\
            ❌ Failed: {}\n\
            💰 Total reclaimed: *{:.9} SOL*\n\n\
            _Automated batch processing completed_",
            emoji, successful, failed, total_sol
        );

        self.send_message(&message).await;
    }

    /// Send error notification
    pub async fn notify_error(&self, error_msg: &str) {
        if !self.enabled {
            return;
        }

        let message = format!(
            "⚠️ *Error Occurred*\n\n\
            {}\n\n\
            _Please check the system logs_",
            error_msg
        );

        self.send_message(&message).await;
    }

    /// Send high-value alert (only if threshold exceeded)
    pub async fn notify_high_value_reclaim(&self, pubkey: &str, amount: u64, threshold_sol: f64) {
        if !self.enabled {
            return;
        }

        let sol_amount = crate::solana::rent::RentCalculator::lamports_to_sol(amount);
        
        if sol_amount < threshold_sol {
            return; // Don't notify if below threshold
        }

        let message = format!(
            "💎 *High-Value Reclaim*\n\n\
            Account: `{}`\n\
            Amount: *{:.9} SOL*\n\n\
            ⚠️ _This exceeds your alert threshold of {:.2} SOL_",
            Self::format_pubkey(pubkey),
            sol_amount,
            threshold_sol
        );

        self.send_message(&message).await;
    }

    /// Send daily summary
    pub async fn notify_daily_summary(&self, total_reclaimed: u64, operations: usize) {
        if !self.enabled {
            return;
        }

        let sol_amount = crate::solana::rent::RentCalculator::lamports_to_sol(total_reclaimed);
        let message = format!(
            "📈 *Daily Summary*\n\n\
            Operations: {}\n\
            Total reclaimed: *{:.9} SOL*\n\n\
            _Last 24 hours of activity_",
            operations,
            sol_amount
        );

        self.send_message(&message).await;
    }

    /// Format pubkey for display
    fn format_pubkey(pubkey: &str) -> String {
        if pubkey.len() <= 12 {
            pubkey.to_string()
        } else {
            format!("{}...{}", &pubkey[..8], &pubkey[pubkey.len()-8..])
        }
    }
}