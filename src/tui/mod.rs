use std::io;
use anyhow::Result;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    event::KeyCode,
};
use ratatui::{
    backend::CrosstermBackend,
    Terminal,
};
use tracing::debug;

use crate::config::Config;

mod state;
mod event;
mod task;
mod ui;

use state::State;
use event::{EventLoop, Event, Command};
use task::TaskManager;

/// Run the TUI application
pub async fn run_tui(config: Config) -> Result<()> {
    debug!("Initializing TUI");
    // Initialize logging to file instead of stdout
    // let log_file = std::fs::File::create("nexus.log")?;
    // tracing_subscriber::fmt()
    //     .with_writer(std::sync::Arc::new(log_file))
    //     .with_ansi(false)
    //     .init();
    
    // Setup terminal with mouse support
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(
        stdout,
        EnterAlternateScreen,
        crossterm::event::EnableMouseCapture
    )?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    
    // Initialize components
    let network = format!("{:?}", config.solana.network);
    let treasury = config.treasury_wallet()?.to_string();
    
    let mut state = State::new(network, treasury);
    let mut event_loop = EventLoop::new();
    let max_txns = config.reclaim.scan_max_transactions;
    
    // Initialize backend components
    let rpc_client = crate::solana::SolanaRpcClient::new(
        &config.solana.rpc_url,
        config.commitment_config(),
        config.solana.rate_limit_delay_ms,
    );
    
    let operator_pubkey = config.operator_pubkey()?;
    let monitor = crate::kora::KoraMonitor::new(rpc_client.clone(), operator_pubkey);
    
    let eligibility_checker = crate::reclaim::EligibilityChecker::new(rpc_client.clone(), config.clone());
    
    let reclaim_engine = match config.load_treasury_keypair() {
        Ok(keypair) => {
            let treasury = config.treasury_wallet()?;
            Some(crate::reclaim::ReclaimEngine::new(
                rpc_client.clone(),
                treasury,
                keypair,
                config.reclaim.dry_run,
            ))
        }
        Err(_) => {
            state.apply(state::Action::Error(
                "Warning: Treasury keypair not loaded - execute disabled".to_string()
            ));
            None
        }
    };
    
    let db = crate::storage::Database::new(&config.database.path)?;
    
    let task_manager = TaskManager::new(
        rpc_client.clone(),
        monitor,
        eligibility_checker,
        reclaim_engine,
        db,
    );
    
    // Start RPC health monitoring
    let action_tx = event_loop.get_action_sender();
    task_manager.spawn_health_check(action_tx.clone());
    
    // Load whitelist
    if let Ok(whitelist) = load_whitelist() {
        for addr in whitelist {
            state.whitelist.insert(addr);
        }
    }
    
    // Main event loop
    let result = run_event_loop(
        &mut terminal,
        &mut state,
        &mut event_loop,
        &task_manager,
        max_txns,
    )
    .await;
    
    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        crossterm::event::DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    
    result
}

/// Main event loop - runs at 30 FPS
async fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut State,
    event_loop: &mut EventLoop,
    task_manager: &TaskManager,
    max_txns: usize,
) -> Result<()> {
    let mut should_quit = false;
    
    while !should_quit {
        // Render at 30 FPS
        terminal.draw(|f| ui::render(f, state))?;
        
        // Handle next event
        if let Some(event) = event_loop.next().await {
            match event {
                Event::Tick => {
                    // Just render on tick
                }
                
                Event::Key(key_event) => {
                    debug!("Received key event: {:?}", key_event);
                    // Special handling for help overlay
                    if state.show_help && (key_event.code == KeyCode::Esc || key_event.code == KeyCode::Char('?')) {
                        state.show_help = false;
                        continue;
                    }
                    
                    if let Some(command) = Command::from_key(key_event) {
                        debug!("Dispatching command: {:?}", command);
                        should_quit = handle_command(command, state, task_manager, event_loop, max_txns).await?;
                    }
                }
                
                Event::Mouse(kind, _x, _y) => {
                    if let Some(command) = Command::from_mouse(kind) {
                        debug!("Dispatching mouse command: {:?}", command);
                        should_quit = handle_command(command, state, task_manager, event_loop, max_txns).await?;
                    }
                }
                
                Event::Resize(width, height) => {
                    debug!("Terminal resize: {}x{}", width, height);
                    state.apply(state::Action::Log(state::LogEntry {
                        timestamp: chrono::Utc::now(),
                        level: state::LogLevel::Warning,
                        message: format!("Terminal resized to {}x{}", width, height),
                    }));
                }
                
                Event::Action(action) => {
                    // Apply state updates from background tasks
                    state.apply(action);
                }
            }
        }
    }
    
    Ok(())
}

/// Handle keyboard commands
async fn handle_command(
    command: Command,
    state: &mut State,
    task_manager: &TaskManager,
    event_loop: &mut EventLoop,
    max_txns: usize,
) -> Result<bool> {
    let action_tx = event_loop.get_action_sender();
    
    match command {
        Command::Quit => {
            return Ok(true);
        }
        
        Command::ToggleHelp => {
            state.show_help = !state.show_help;
        }
        
        Command::ToggleExpanded => {
            state.show_expanded_details = !state.show_expanded_details;
        }
        
        Command::Scan => {
            if !state.is_scanning {
                debug!("Starting scan with max_txns={}", max_txns);
                task_manager.spawn_scan(action_tx, max_txns);
            } else {
                debug!("Scan request ignored: scan already in progress");
            }
        }
        
        Command::DryRun => {
            if let Some(account) = state.get_selected_account() {
                if account.status == state::AccountStatus::Eligible 
                    && !state.whitelist.contains(&account.address.to_string()) {
                    task_manager.spawn_dry_run(action_tx, account.address.to_string());
                }
            }
        }
        
        Command::Execute => {
            // Safety checks
            if !state.can_execute_selected() {
                state.apply(state::Action::Error(
                    "Execute blocked: Dry-run required first!".to_string()
                ));
                return Ok(false);
            }
            
            if let Some(account) = state.get_selected_account() {
                if account.status == state::AccountStatus::Eligible {
                    task_manager.spawn_reclaim(action_tx, account.address.to_string());
                }
            }
        }
        
        Command::Export => {
            if !state.accounts.is_empty() {
                let accounts = state.accounts.clone();
                task_manager.spawn_export(action_tx, accounts);
            }
        }
        
        Command::Whitelist => {
            if let Some(account) = state.get_selected_account() {
                if account.status != state::AccountStatus::Whitelisted {
                    task_manager.spawn_whitelist(action_tx, account.address.to_string());
                }
            }
        }
        
        Command::NavigateUp | Command::MouseScrollUp => {
            state.select_previous();
        }
        
        Command::NavigateDown | Command::MouseScrollDown => {
            state.select_next();
        }
        
        Command::PageUp => {
            for _ in 0..10 {
                state.select_previous();
            }
        }
        
        Command::PageDown => {
            for _ in 0..10 {
                state.select_next();
            }
        }
        
        Command::Refresh => {
            // Could trigger a stats refresh
            state.apply(state::Action::Log(state::LogEntry {
                timestamp: chrono::Utc::now(),
                level: state::LogLevel::Info,
                message: "Stats refreshed".to_string(),
            }));
        }
        
        Command::ToggleMode => {
            state.current_mode = match state.current_mode {
                state::Mode::Monitor => state::Mode::DryRun,
                state::Mode::DryRun => state::Mode::Execute,
                state::Mode::Execute => state::Mode::Monitor,
            };
        }
    }
    
    Ok(false)
}

/// Load whitelist from file
fn load_whitelist() -> Result<Vec<String>> {
    use std::fs::File;
    use std::io::Read;
    
    let mut file = File::open("whitelist.json")?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    
    let whitelist: Vec<String> = serde_json::from_str(&contents)?;
    Ok(whitelist)
}
