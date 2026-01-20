use colored::Colorize;

/// Format lamports as SOL string with color
pub fn format_sol(lamports: u64) -> String {
    format!("{:.9} SOL", crate::solana::rent::RentCalculator::lamports_to_sol(lamports))
        .yellow()
        .to_string()
}

/// Format pubkey truncated for display
pub fn format_pubkey(pubkey: &str) -> String {
    if pubkey.len() <= 12 {
        pubkey.to_string()
    } else {
        format!("{}...{}", &pubkey[..6], &pubkey[pubkey.len()-6..])
    }
}

/// Format timestamp in human-readable format
pub fn format_timestamp(timestamp: &chrono::DateTime<chrono::Utc>) -> String {
    timestamp.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

/// Simple rate limiter using token bucket algorithm
pub struct RateLimiter {
    delay: std::time::Duration,
    last_call: std::sync::Mutex<Option<std::time::Instant>>,
}

impl RateLimiter {
    pub fn new(delay_ms: u64) -> Self {
        Self {
            delay: std::time::Duration::from_millis(delay_ms),
            last_call: std::sync::Mutex::new(None),
        }
    }
    
    pub async fn wait(&self) {
        let mut last = self.last_call.lock().unwrap();
        
        if let Some(last_time) = *last {
            let elapsed = last_time.elapsed();
            if elapsed < self.delay {
                let remaining = self.delay - elapsed;
                drop(last); // Release lock before sleeping
                tokio::time::sleep(remaining).await;
                *self.last_call.lock().unwrap() = Some(std::time::Instant::now());
            } else {
                *last = Some(std::time::Instant::now());
            }
        } else {
            *last = Some(std::time::Instant::now());
        }
    }
}

/// Prompt user for yes/no confirmation
pub fn confirm_action(prompt: &str) -> bool {
    use std::io::{self, Write};
    
    print!("{} (y/N): ", prompt);
    io::stdout().flush().unwrap();
    
    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    
    matches!(input.trim().to_lowercase().as_str(), "y" | "yes")
}

/// Print a formatted table border
pub fn print_table_border(width: usize) {
    println!("{}", "=".repeat(width));
}

/// Print a table row with columns
pub fn print_table_row(columns: &[&str], widths: &[usize]) {
    let mut row = String::new();
    for (i, col) in columns.iter().enumerate() {
        if i < widths.len() {
            row.push_str(&format!("{:<width$}  ", col, width = widths[i]));
        }
    }
    println!("{}", row.trim_end());
}
