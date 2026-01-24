use crate::solana::rent::RentCalculator;

/// Format SOL for Telegram (no ANSI colors)
pub fn format_sol_tg(lamports: u64) -> String {
    format!("{:.9} SOL", RentCalculator::lamports_to_sol(lamports))
}

/// Format pubkey for Telegram with monospace
#[allow(dead_code)]
pub fn format_pubkey_tg(pubkey: &str) -> String {
    if pubkey.len() <= 12 {
        format!("`{}`", pubkey)
    } else {
        format!("`{}...{}`", &pubkey[..8], &pubkey[pubkey.len()-8..])
    }
}

/// Format account info for Telegram
#[allow(dead_code)]
pub fn format_account_tg(
    pubkey: &str,
    balance: u64,
    created: &chrono::DateTime<chrono::Utc>,
    status: &str
) -> String {
    format!(
        "🔹 {}\n💰 {}\n📅 {}\n📊 {}",
        format_pubkey_tg(pubkey),
        format_sol_tg(balance),
        created.format("%Y-%m-%d %H:%M UTC"),
        status
    )
}