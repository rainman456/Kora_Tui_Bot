use teloxide::prelude::*;
use std::sync::Arc;
use crate::telegram::bot::BotState;

/// Handle callback queries (inline buttons)
pub async fn handle_callback(bot: Bot, q: CallbackQuery, state: Arc<BotState>) -> ResponseResult<()> {
     // Check authorization
    let user_id = q.from.id.0;
    if let Some(telegram_config) = &state.config.telegram {
        if !telegram_config.authorized_users.is_empty() && !telegram_config.authorized_users.contains(&user_id) {
            bot.answer_callback_query(q.id).text("⛔ Not authorized").show_alert(true).await?;
            return Ok(());
        }
    }

    if let Some(data) = q.data {
        // Implement callbacks for pagination, confirmation etc.
        // For now just acknowledge
        bot.answer_callback_query(q.id).text(format!("Received: {}", data)).await?;
    }

    Ok(())
}
