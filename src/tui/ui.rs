use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Alignment},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Row, Table, Tabs},
    Frame, Terminal,
};
use std::io;
use crate::tui::app::{App, Screen};
use crate::config::Config;
use crate::error::Result;

pub async fn run_tui(config: Config) -> Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    
    // Create app
    let mut app = App::new(config).await?;
    
    // Initial data load
    app.refresh_stats().await?;
    
    // Run app
    let res = run_app(&mut terminal, &mut app).await;
    
    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;
    
    res
}

async fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|f| ui(f, app))?;
        
        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        app.should_quit = true;
                    }
                    KeyCode::Tab => app.next_screen(),
                    KeyCode::BackTab => app.previous_screen(),
                    KeyCode::Down | KeyCode::Char('j') => app.next_item(),
                    KeyCode::Up | KeyCode::Char('k') => app.previous_item(),
                    KeyCode::Char('s') => {
                        app.scan_accounts().await?;
                    }
                    KeyCode::Char('r') => {
                        app.refresh_stats().await?;
                    }
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        app.should_quit = true;
                    }
                    KeyCode::Char('t') => {
                        // Toggle Telegram
                        app.toggle_telegram();
                    }
                    KeyCode::Char('T') => {
                        // Test Telegram (Shift+T)
                        app.test_telegram().await;
                    }
                    KeyCode::Enter => {
                        if app.current_screen == Screen::Accounts {
                            app.reclaim_selected().await?;
                        }
                    }
                    KeyCode::Char('b') => {
                        if app.current_screen == Screen::Accounts {
                            app.batch_reclaim().await?;
                        }
                    }
                    _ => {}
                }
            }
        } else {
            // Timeout expired (tick)
            app.on_tick().await;
        }
        
        if app.should_quit {
            break;
        }
    }
    
    Ok(())
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(0),
            Constraint::Length(3),
        ])
        .split(f.size());
    
    // Header
    render_header(f, chunks[0], app);
    
    // Content
    match app.current_screen {
        Screen::Dashboard => render_dashboard(f, chunks[1], app),
        Screen::Accounts => render_accounts(f, chunks[1], app),
        Screen::Operations => render_operations(f, chunks[1], app),
        Screen::Settings => render_settings(f, chunks[1], app),
    }
    
    // Status bar
    render_status(f, chunks[2], app);
}

fn render_header(f: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let title = Line::from(vec![
        Span::raw("⚡ "),
        Span::styled("Kora Rent Reclaim", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" | "),
        Span::styled(format!("{:?}", app.config.solana.network), Style::default().fg(Color::Green)),
    ]);
    
    let block = Block::default().borders(Borders::ALL);
    let paragraph = Paragraph::new(title).block(block).alignment(Alignment::Center);
    f.render_widget(paragraph, area);
}

fn render_status(f: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let screens = vec!["Dashboard", "Accounts", "Operations", "Settings"];
    let screen_idx = match app.current_screen {
        Screen::Dashboard => 0,
        Screen::Accounts => 1,
        Screen::Operations => 2,
        Screen::Settings => 3,
    };
    
    let help_text = match app.current_screen {
        Screen::Dashboard => " s:Scan | r:Refresh | t:Toggle TG | T:Test TG ",
        Screen::Accounts => " Enter:Reclaim | b:Batch | s:Scan | t:Toggle TG ",
        Screen::Operations => " r:Refresh ",
        Screen::Settings => " t:Toggle TG | T:Test TG ",
    };
    
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);
    
    let tabs = Tabs::new(screens)
        .block(Block::default().borders(Borders::LEFT | Borders::TOP | Borders::BOTTOM))
        .select(screen_idx)
        .style(Style::default().fg(Color::White))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));
    
    f.render_widget(tabs, chunks[0]);
    
    let help = Paragraph::new(Line::from(Span::styled(
        help_text,
        Style::default().fg(Color::DarkGray)
    )))
    .block(Block::default().borders(Borders::ALL));
    
    f.render_widget(help, chunks[1]);
}

fn render_dashboard(f: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),  // Stats row 1
            Constraint::Length(3),  // Stats row 2 (Telegram)
            Constraint::Length(3),  // Alerts (NEW)
            Constraint::Min(0)      // Logs
        ])
        .split(area);
    
    // Stats row 1
    let stats_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(25); 4])
        .split(chunks[0]);
    
    let stats = [
        ("Total", app.total_accounts.to_string(), Color::Cyan),
        ("Eligible", app.eligible_accounts.to_string(), Color::Green),
        ("Locked", format!("{:.4} SOL", app.total_locked as f64 / 1_000_000_000.0), Color::Yellow),
        ("Reclaimed", format!("{:.4} SOL", app.total_reclaimed as f64 / 1_000_000_000.0), Color::Green),
    ];
    
    for (i, (label, value, color)) in stats.iter().enumerate() {
        let text = vec![
            Line::from(Span::raw(*label)),
            Line::from(Span::styled(value, Style::default().fg(*color).add_modifier(Modifier::BOLD))),
        ];
        let block = Block::default().borders(Borders::ALL);
        let para = Paragraph::new(text).block(block).alignment(Alignment::Center);
        f.render_widget(para, stats_chunks[i]);
    }
    
    // Telegram status row
    let telegram_color = if app.telegram_enabled {
        Color::Green
    } else if app.telegram_configured {
        Color::Yellow
    } else {
        Color::Red
    };
    
    let telegram_icon = if app.telegram_enabled { "✓" } else { "✗" };
    
    let telegram_text = vec![
        Line::from(vec![
            Span::styled(
                format!("{} Telegram Notifications: ", telegram_icon),
                Style::default().fg(telegram_color).add_modifier(Modifier::BOLD)
            ),
            Span::styled(
                &app.telegram_status,
                Style::default().fg(telegram_color)
            ),
        ]),
        Line::from(Span::styled(
            "Press 't' to toggle | 'T' to test",
            Style::default().fg(Color::DarkGray)
        )),
    ];
    
    let telegram_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(telegram_color));
    let telegram_para = Paragraph::new(telegram_text)
        .block(telegram_block)
        .alignment(Alignment::Center);
    f.render_widget(telegram_para, chunks[1]);
    
    // Alerts
    let alert_text = if app.alerts.is_empty() {
        vec![Line::from(Span::styled("No active alerts", Style::default().fg(Color::Gray)))]
    } else {
        app.alerts.iter().map(|alert| {
            Line::from(Span::styled(alert, Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)))
        }).collect()
    };
    
    let alerts_block = Block::default().borders(Borders::ALL).title("Alerts");
    let alerts_para = Paragraph::new(alert_text).block(alerts_block);
    f.render_widget(alerts_para, chunks[2]);
    
    // Logs
    let logs: Vec<ListItem> = app.logs.iter().rev().take(20).map(|log| {
        ListItem::new(Line::from(Span::raw(log)))
    }).collect();
    
    let logs_list = List::new(logs)
        .block(Block::default().borders(Borders::ALL).title("Activity Log"));
    f.render_widget(logs_list, chunks[3]);
}

fn render_accounts(f: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    // ✅ FIX: Add Created column to the table
    let header = Row::new(vec!["Pubkey", "Balance", "Created", "Status"])
        .style(Style::default().fg(Color::Yellow))
        .bottom_margin(1);
    
    let rows: Vec<Row> = app.accounts.iter().map(|acc| {
        let color = if acc.eligible { Color::Green } else { Color::Gray };
        Row::new(vec![
            format!("{}...{}", &acc.pubkey[..8], &acc.pubkey[acc.pubkey.len()-8..]),
            format!("{:.4}", acc.balance as f64 / 1_000_000_000.0),
            
            acc.created.format("%m-%d %H:%M").to_string(),
            acc.status.clone(),
        ]).style(Style::default().fg(color))
    }).collect();
    
   
    let table = Table::new(
        rows, 
        [
            Constraint::Percentage(40),  // Pubkey
            Constraint::Percentage(20),  // Balance
            Constraint::Percentage(20),  // Created (NEW)
            Constraint::Percentage(20),  // Status
        ]
    )
        .header(header)
        .block(Block::default().borders(Borders::ALL).title("Accounts (Enter: Reclaim | b: Batch | s: Scan)"))
        .highlight_style(Style::default().bg(Color::DarkGray));
    
    let mut state = ratatui::widgets::TableState::default();
    state.select(Some(app.selected_index));
    f.render_stateful_widget(table, area, &mut state);
}
fn render_operations(f: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let header = Row::new(vec!["Time", "Account", "Amount", "Signature"])
        .style(Style::default().fg(Color::Yellow))
        .bottom_margin(1);
    
    let rows: Vec<Row> = app.operations.iter().map(|op| {
        Row::new(vec![
            op.timestamp.format("%m-%d %H:%M").to_string(),
            format!("{}...", &op.account[..8]),
            format!("{:.4}", op.amount as f64 / 1_000_000_000.0),
            format!("{}...", &op.signature[..8]),
        ])
    }).collect();
    
    let table = Table::new(
        rows,
        [
            Constraint::Percentage(20),
            Constraint::Percentage(30),
            Constraint::Percentage(20),
            Constraint::Percentage(30)
        ]
    )
        .header(header)
        .block(Block::default().borders(Borders::ALL).title("Reclaim History"));
    
    f.render_widget(table, area);
}

fn render_settings(f: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let mut settings = vec![
        format!("RPC: {}", app.config.solana.rpc_url),
        format!("Network: {:?}", app.config.solana.network),
        format!("Min Inactive Days: {}", app.config.reclaim.min_inactive_days),
        format!("Batch Size: {}", app.config.reclaim.batch_size),
        format!("Dry Run: {}", app.config.reclaim.dry_run),
        String::new(), // Separator
        format!("=== Telegram Settings ==="),
    ];
    
    if let Some(ref tg_config) = app.config.telegram {
        settings.push(format!("Bot Token: {}...", &tg_config.bot_token[..10]));
        settings.push(format!("Authorized Users: {}", tg_config.authorized_users.len()));
        settings.push(format!("Notifications: {}", if tg_config.notifications_enabled { "Enabled" } else { "Disabled" }));
        settings.push(format!("Alert Threshold: {} SOL", tg_config.alert_threshold_sol));
        settings.push(String::new());
        settings.push(format!("Status: {}", app.telegram_status));
    } else {
        settings.push("Not configured".to_string());
        settings.push("Add [telegram] section to config.toml".to_string());
    }
    
    let items: Vec<ListItem> = settings.into_iter().map(|s| {
        let color = if s.starts_with("===") {
            Color::Cyan
        } else if s.contains("Enabled") || s.contains("Active") {
            Color::Green
        } else if s.contains("Disabled") || s.contains("Not configured") {
            Color::Yellow
        } else {
            Color::White
        };
        
        ListItem::new(Line::from(Span::styled(s, Style::default().fg(color))))
    }).collect();
    
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Configuration (t: Toggle Telegram | T: Test)"));
    f.render_widget(list, area);
}