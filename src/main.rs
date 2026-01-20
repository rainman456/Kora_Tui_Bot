mod solana;
mod kora;
mod reclaim;
mod storage;
mod tui;
mod cli;
mod config;
mod error;
mod utils;

use clap::Parser;
use cli::{Cli, Commands};
use config::Config;
use tracing::{info, error};
use colored::*;

// TUI imports
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;

#[tokio::main]
async fn main() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("kora_reclaim=debug,info")
        .init();
    
    // Parse CLI arguments
    let cli = Cli::parse();
    
    // Load configuration
    let config = match Config::load() {
        Ok(cfg) => cfg,
        Err(e) => {
            error!("Failed to load configuration: {}", e);
            std::process::exit(1);
        }
    };
    
    // Execute command
    let result = match cli.command {
        Commands::Tui => {
            run_tui(config).await
        }
        
        Commands::Scan { verbose } => {
            info!("Scanning for eligible accounts...");
            scan_accounts(&config, verbose).await
        }
        
        Commands::Reclaim { pubkey, yes } => {
            info!("Reclaiming account: {}", pubkey);
            reclaim_account(&config, &pubkey, yes).await
        }
        
        Commands::Auto { interval } => {
            info!("Starting automated reclaim service (interval: {}s)", interval);
            run_auto_service(&config, interval).await
        }
        
        Commands::Stats => {
            info!("Generating statistics...");
            show_stats(&config).await
        }
        
        Commands::Init => {
            info!("Initializing...");
            initialize(&config).await
        }
    };
    
    if let Err(e) = result {
        error!("{}", format!("Error: {}", e).red());
        std::process::exit(1);
    }
}

async fn run_tui(config: Config) -> error::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    
    // Create app
    let mut app = tui::app::App::new(config).await?;
    
    // Load initial data
    app.refresh_data().await?;
    app.add_log(tui::app::LogLevel::Info, "TUI started".to_string());
    
    // Create event handler
    let mut events = tui::event::EventHandler::new(std::time::Duration::from_millis(250));
    
    // Main loop
    loop {
        terminal.draw(|f| {
            tui::ui::render_ui(f, &mut app);
        })?;
        
        if let Some(event) = events.next().await {
            match event {
                tui::event::Event::Tick => {
                    // Background updates
                }
                tui::event::Event::Key(key) => {
                    use crossterm::event::{KeyCode, KeyModifiers};
                    
                    match (key.code, key.modifiers) {
                        (KeyCode::Char('q'), _) | (KeyCode::Esc, _) => {
                            app.quit();
                        }
                        (KeyCode::Tab, KeyModifiers::NONE) => {
                            app.next_screen();
                        }
                        (KeyCode::BackTab, _) => {
                            app.previous_screen();
                        }
                        (KeyCode::Up, _) | (KeyCode::Char('k'), _) => {
                            app.select_previous_account();
                        }
                        (KeyCode::Down, _) | (KeyCode::Char('j'), _) => {
                            app.select_next_account();
                        }
                        (KeyCode::Char('r'), _) => {
                            app.add_log(tui::app::LogLevel::Info, "Refreshing data...".to_string());
                            app.refresh_data().await?;
                            app.add_log(tui::app::LogLevel::Success, "Data refreshed".to_string());
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        
        if app.should_quit {
            break;
        }
    }
    
    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    
    Ok(())
}

// ... rest of existing functions (scan_accounts, reclaim_account, etc.)
async fn scan_accounts(config: &Config, verbose: bool) -> error::Result<()> {
    println!("{}", "Scanning for eligible accounts...".cyan());
    Ok(())
}

async fn reclaim_account(config: &Config, pubkey: &str, yes: bool) -> error::Result<()> {
    println!("{}", format!("Reclaiming account: {}", pubkey).cyan());
    Ok(())
}

async fn run_auto_service(config: &Config, interval: u64) -> error::Result<()> {
    println!("{}", "Starting automated service...".green());
    loop {
        info!("Running reclaim cycle...");
        tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;
    }
}

async fn show_stats(config: &Config) -> error::Result<()> {
    println!("{}", "=== Kora Rent Reclaim Statistics ===".cyan().bold());
    Ok(())
}

async fn initialize(config: &Config) -> error::Result<()> {
    println!("{}", "Initializing Kora Rent Reclaim Bot...".green());
    let db = storage::db::Database::new(&config.database.path)?;
    println!("{}", "✓ Database initialized".green());
    println!("{}", "✓ Configuration loaded".green());
    println!("\n{}", "Ready to use! Try running:".cyan());
    println!("  {} to launch TUI dashboard", "kora-reclaim tui".yellow());
    Ok(())
}