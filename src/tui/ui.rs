use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, BorderType, Cell, Clear, List, ListItem, Paragraph, Row, Table, TableState, Wrap},
    Frame,
};

use super::state::{State, AccountStatus, LogLevel, HealthStatus};

// High-contrast color palette
const COLOR_PRIMARY: Color = Color::Cyan;
const COLOR_SUCCESS: Color = Color::Green;
const COLOR_WARNING: Color = Color::Yellow;
const COLOR_DANGER: Color = Color::Red;
const COLOR_INFO: Color = Color::Blue;
const COLOR_MUTED: Color = Color::DarkGray;
const COLOR_TEXT: Color = Color::White;
const COLOR_BG_HIGHLIGHT: Color = Color::Rgb(40, 40, 60);
const COLOR_BORDER_ACTIVE: Color = Color::Cyan;
const COLOR_BORDER_INACTIVE: Color = Color::DarkGray;

/// Render the complete Nexus layout
pub fn render(f: &mut Frame, state: &State) {
    let size = f.size();

    if size.height < 24 {
        render_compact(f, size, state);
        return;
    }
    
    // Main layout structure
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Length(5),  // Summary Bar
            Constraint::Min(10),    // Main Workspace
            Constraint::Length(7),  // Activity Log
            Constraint::Length(3),  // Footer
        ])
        .split(size);
    
    render_header(f, chunks[0], state);
    render_summary_bar(f, chunks[1], state);
    render_workspace(f, chunks[2], state);
    render_activity_log(f, chunks[3], state);
    render_footer(f, chunks[4], state);
    
    // Render help overlay on top if active
    if state.show_help {
        render_help_overlay(f, size);
    }
}

fn render_compact(f: &mut Frame, size: Rect, state: &State) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(5),     // Main Workspace
            Constraint::Length(3),  // Footer
        ])
        .split(size);

    render_header(f, chunks[0], state);
    render_workspace(f, chunks[1], state);
    render_footer(f, chunks[2], state);

    if state.show_help {
        render_help_overlay(f, size);
    }
}

/// Header: Network, Node, Mode, RPC Health
fn render_header(f: &mut Frame, area: Rect, state: &State) {
    let health_color = match state.rpc_health.status {
        HealthStatus::Healthy => COLOR_SUCCESS,
        HealthStatus::Degraded => COLOR_WARNING,
        HealthStatus::Down => COLOR_DANGER,
    };
    
    let mode_color = match state.current_mode {
        super::state::Mode::Monitor => COLOR_PRIMARY,
        super::state::Mode::DryRun => COLOR_WARNING,
        super::state::Mode::Execute => COLOR_DANGER,
    };
    
    // Health indicator icon
    let health_icon = match state.rpc_health.status {
        HealthStatus::Healthy => "●",
        HealthStatus::Degraded => "◐",
        HealthStatus::Down => "○",
    };
    
    let header_line = Line::from(vec![
        Span::styled("⚡ ", Style::default().fg(COLOR_WARNING)),
        Span::styled("KORA NEXUS", Style::default().fg(COLOR_PRIMARY).add_modifier(Modifier::BOLD)),
        Span::styled(" │ ", Style::default().fg(COLOR_MUTED)),
        Span::styled(&state.network, Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
        Span::styled(" │ ", Style::default().fg(COLOR_MUTED)),
        Span::styled("Mode: ", Style::default().fg(COLOR_MUTED)),
        Span::styled(format!("{:?}", state.current_mode), Style::default().fg(mode_color).add_modifier(Modifier::BOLD)),
        Span::styled(" │ ", Style::default().fg(COLOR_MUTED)),
        Span::styled(format!("{} ", health_icon), Style::default().fg(health_color).add_modifier(Modifier::BOLD)),
        Span::styled("RPC: ", Style::default().fg(COLOR_MUTED)),
        Span::styled(
            format!("{}ms", state.rpc_health.latency_ms),
            Style::default().fg(health_color).add_modifier(Modifier::BOLD)
        ),
        Span::styled(" ", Style::default().fg(COLOR_MUTED)),
        Span::styled(state.rpc_health.status.display(), Style::default().fg(health_color)),
        Span::styled(" @ ", Style::default().fg(COLOR_MUTED)),
        Span::styled(
            state.rpc_health.last_check.format("%H:%M:%S").to_string(),
            Style::default().fg(COLOR_MUTED)
        ),
        Span::styled(" │ ", Style::default().fg(COLOR_MUTED)),
        Span::styled("? ", Style::default().fg(COLOR_INFO)),
        Span::styled("Help", Style::default().fg(COLOR_MUTED)),
    ]);
    
    let header_style = if state.rpc_health.status == HealthStatus::Down {
        Style::default().bg(COLOR_DANGER).fg(COLOR_TEXT)
    } else {
        Style::default()
    };
    
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_PRIMARY))
        .style(header_style);
    
    let paragraph = Paragraph::new(header_line)
        .block(block)
        .alignment(Alignment::Center);
    
    f.render_widget(paragraph, area);
}

/// Summary Bar: 5 overview cards
fn render_summary_bar(f: &mut Frame, area: Rect, state: &State) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(20); 5])
        .split(area);
    
    // Calculate percentages for visual bars
    let eligible_pct = if state.total_accounts > 0 {
        (state.eligible_accounts as f64 / state.total_accounts as f64 * 100.0) as u16
    } else {
        0
    };
    
    let cards = [
        ("📊 TOTAL", state.total_accounts.to_string(), "".to_string(), COLOR_PRIMARY, COLOR_MUTED),
        ("🔒 LOCKED", format!("{:.4} SOL", state.total_locked_sol), "".to_string(), COLOR_WARNING, COLOR_MUTED),
        ("✓ ELIGIBLE", state.eligible_accounts.to_string(), format!("{}%", eligible_pct), COLOR_SUCCESS, COLOR_SUCCESS),
        ("💰 RECLAIMED", format!("{:.4} SOL", state.total_reclaimed_sol), "".to_string(), COLOR_SUCCESS, COLOR_SUCCESS),
        ("⚠ AT-RISK", format!("{:.4} SOL", state.at_risk_sol), ">30d".to_string(), COLOR_DANGER, COLOR_DANGER),
    ];
    
    for (i, (label, value, subtitle, fg_color, border_color)) in cards.iter().enumerate() {
        let mut text_lines = vec![
            Line::from(Span::styled(*label, Style::default().fg(COLOR_MUTED).add_modifier(Modifier::DIM))),
            Line::from(Span::styled(value, Style::default().fg(*fg_color).add_modifier(Modifier::BOLD))),
        ];
        
        if !subtitle.is_empty() {
            //text_lines.push(Line::from(Span::styled(*subtitle, Style::default().fg(COLOR_MUTED))));
            text_lines.push(Line::from(Span::styled(subtitle.clone(), Style::default().fg(COLOR_MUTED))));

        }
        
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(*border_color));
        
        let para = Paragraph::new(text_lines)
            .block(block)
            .alignment(Alignment::Center);
        
        f.render_widget(para, chunks[i]);
    }
}

/// Main Workspace: Monitor table (70%) + Decision panel (30%)
fn render_workspace(f: &mut Frame, area: Rect, state: &State) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);
    
    render_monitor_table(f, chunks[0], state);
    render_decision_panel(f, chunks[1], state);
}

/// Monitor Table: Account list with selection
fn render_monitor_table(f: &mut Frame, area: Rect, state: &State) {
    let header = Row::new(vec![
        Cell::from("Address"),
        Cell::from("Program"),
        Cell::from("Age"),
        Cell::from("Rent"),
        Cell::from("Last Tx"),
        Cell::from("Status"),
    ])
    .style(Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD | Modifier::UNDERLINED))
    .bottom_margin(0);
    
    let rows: Vec<Row> = state.accounts.iter().enumerate().map(|(idx, acc)| {
        // Status-based styling with high contrast
        let (style, status_text, icon) = match acc.status {
            AccountStatus::Eligible => (
                Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD),
                acc.status.display(),
                "✓"
            ),
            AccountStatus::Whitelisted => (
                Style::default().fg(COLOR_MUTED).add_modifier(Modifier::DIM),
                acc.status.display(),
                "🔒"
            ),
            AccountStatus::Reclaimed => (
                Style::default().fg(COLOR_PRIMARY),
                acc.status.display(),
                "✓"
            ),
            AccountStatus::Processing => (
                Style::default().fg(COLOR_INFO).add_modifier(Modifier::SLOW_BLINK),
                acc.status.display(),
                "⟳"
            ),
            AccountStatus::Failed => (
                Style::default().fg(COLOR_DANGER),
                acc.status.display(),
                "✗"
            ),
            AccountStatus::Active => (
                Style::default().fg(COLOR_MUTED),
                acc.status.display(),
                "○"
            ),
        };
        
        // Compact address display
        let addr_str = format!("{}..{}", 
            &acc.address.to_string()[..4], 
            &acc.address.to_string()[acc.address.to_string().len()-4..]
        );
        
        // Smart age formatting
        let age_str = if acc.age_days > 365 {
            format!("{}y", acc.age_days / 365)
        } else if acc.age_days > 30 {
            format!("{}mo", acc.age_days / 30)
        } else {
            format!("{}d", acc.age_days)
        };
        
        // Compact date
        let last_tx_str = acc.last_tx.format("%m/%d").to_string();
        
        // Highlight row based on selection
        let row_style = if idx == state.selected_index {
            style.bg(COLOR_BG_HIGHLIGHT)
        } else {
            style
        };
        
        Row::new(vec![
            Cell::from(addr_str),
            Cell::from(acc.program.chars().take(10).collect::<String>()),
            Cell::from(age_str),
            Cell::from(format!("{:.4}", acc.rent_sol)),
            Cell::from(last_tx_str),
            Cell::from(format!("{} {}", icon, status_text)),
        ])
        .style(row_style)
        .height(1)
    }).collect();
    
    let loading_indicator = if state.is_scanning {
        " ⟳ SCANNING..."
    } else if state.is_processing {
        " ⟳ PROCESSING..."
    } else {
        ""
    };
    
    let title = format!("📊 Monitor{} (↑↓ nav, mouse scroll)", loading_indicator);
    
    let table = Table::new(
        rows,
        [
            Constraint::Length(10),  // Address
            Constraint::Length(12),  // Program
            Constraint::Length(5),   // Age
            Constraint::Length(8),   // Rent
            Constraint::Length(6),   // Last Tx
            Constraint::Min(15),     // Status (flexible)
        ]
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(COLOR_BORDER_ACTIVE))
            .title(title)
    )
    .highlight_style(
        Style::default()
            .bg(COLOR_BG_HIGHLIGHT)
            .fg(COLOR_TEXT)
            .add_modifier(Modifier::BOLD)
    )
    .highlight_symbol("▶ ");
    
    let mut table_state = TableState::default();
    table_state.select(Some(state.selected_index));
    
    f.render_stateful_widget(table, area, &mut table_state);
}

/// Decision Panel: Contextual information for selected account
fn render_decision_panel(f: &mut Frame, area: Rect, state: &State) {
    let selected_account = state.get_selected_account();
    
    let content = if let Some(account) = selected_account {
        let mut lines = vec![
            Line::from(Span::styled("━━━ SELECTED ACCOUNT ━━━", Style::default().fg(COLOR_PRIMARY).add_modifier(Modifier::BOLD))),
            Line::from(""),
        ];
        
        // Compact address display with full on hover
        if state.show_expanded_details {
            lines.push(Line::from(vec![
                Span::styled("Address: ", Style::default().fg(COLOR_MUTED)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::raw(account.address.to_string()[..32].to_string()),
            ]));
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::raw(account.address.to_string()[32..].to_string()),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::styled("Address: ", Style::default().fg(COLOR_MUTED)),
                Span::raw(format!("{}..{}", &account.address.to_string()[..8], &account.address.to_string()[account.address.to_string().len()-8..])),
                Span::styled(" (E for full)", Style::default().fg(COLOR_MUTED).add_modifier(Modifier::DIM)),
            ]));
        }
        
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("Program:  ", Style::default().fg(COLOR_MUTED)),
            Span::styled(&account.program, Style::default().fg(COLOR_TEXT)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Age:      ", Style::default().fg(COLOR_MUTED)),
            Span::styled(format!("{} days", account.age_days), Style::default().fg(if account.age_days > 30 { COLOR_WARNING } else { COLOR_TEXT })),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Rent:     ", Style::default().fg(COLOR_MUTED)),
            Span::styled(format!("{:.6} SOL", account.rent_sol), Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
        ]));
        lines.push(Line::from(vec![
            Span::styled("Status:   ", Style::default().fg(COLOR_MUTED)),
            Span::styled(account.status.display(), Style::default().fg(COLOR_TEXT)),
        ]));
        
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("━━━ ELIGIBILITY ━━━", Style::default().fg(COLOR_INFO).add_modifier(Modifier::BOLD))));
        
        // Word wrap eligibility reason
        let reason_lines = wrap_text(&account.eligibility_reason, 25);
        for reason_line in reason_lines {
            lines.push(Line::from(Span::styled(reason_line, Style::default().fg(COLOR_TEXT))));
        }
        
        lines.push(Line::from(""));
        
        // Add dry-run results if available
        if let Some(dry_run) = state.get_selected_dry_run() {
            lines.push(Line::from(Span::styled("━━━ DRY-RUN RESULTS ━━━", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD))));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("Projected:  ", Style::default().fg(COLOR_MUTED)),
                Span::styled(format!("{:.6} SOL", dry_run.projected_sol), Style::default().fg(COLOR_SUCCESS)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Fee Est:    ", Style::default().fg(COLOR_MUTED)),
                Span::styled(format!("{:.6} SOL", dry_run.estimated_fee), Style::default().fg(COLOR_WARNING)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("━━━━━━━━━━━━━━━━━━━━━━━━", Style::default().fg(COLOR_MUTED)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Net Gain:   ", Style::default().fg(COLOR_MUTED)),
                Span::styled(format!("{:.6} SOL", dry_run.net_gain), Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
            ]));
            lines.push(Line::from(vec![
                Span::styled("Checked:   ", Style::default().fg(COLOR_MUTED)),
                Span::styled(dry_run.timestamp.format("%Y-%m-%d %H:%M:%S").to_string(), Style::default().fg(COLOR_TEXT)),
            ]));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled("✓ ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
                Span::styled("Ready for execution", Style::default().fg(COLOR_TEXT)),
            ]));
        } else if account.status == AccountStatus::Eligible {
            lines.push(Line::from(vec![
                Span::styled("⚠ ", Style::default().fg(COLOR_WARNING)),
                Span::styled("Press ", Style::default().fg(COLOR_MUTED)),
                Span::styled("d", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
                Span::styled(" to run dry-run", Style::default().fg(COLOR_MUTED)),
            ]));
        }
        
        // Safety warnings
        lines.push(Line::from(""));
        if account.status == AccountStatus::Whitelisted {
            lines.push(Line::from(vec![
                Span::styled("🔒 ", Style::default().fg(COLOR_DANGER)),
                Span::styled("WHITELISTED", Style::default().fg(COLOR_DANGER).add_modifier(Modifier::BOLD)),
            ]));
            lines.push(Line::from(Span::styled("Cannot be reclaimed", Style::default().fg(COLOR_MUTED))));
        } else if !state.can_execute_selected() && account.status == AccountStatus::Eligible {
            lines.push(Line::from(vec![
                Span::styled("⚠ ", Style::default().fg(COLOR_WARNING)),
                Span::styled("Dry-run required first", Style::default().fg(COLOR_WARNING)),
            ]));
        }
        
        lines
    } else {
        vec![
            Line::from(""),
            Line::from(Span::styled("No account selected", Style::default().fg(COLOR_MUTED))),
            Line::from(""),
            Line::from(vec![
                Span::styled("Use ", Style::default().fg(COLOR_MUTED)),
                Span::styled("↑↓", Style::default().fg(COLOR_INFO).add_modifier(Modifier::BOLD)),
                Span::styled(" or ", Style::default().fg(COLOR_MUTED)),
                Span::styled("j/k", Style::default().fg(COLOR_INFO).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(Span::styled("to navigate", Style::default().fg(COLOR_MUTED))),
            Line::from(""),
            Line::from(vec![
                Span::styled("Mouse scroll ", Style::default().fg(COLOR_MUTED)),
                Span::styled("enabled", Style::default().fg(COLOR_SUCCESS)),
            ]),
        ]
    };
    
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_BORDER_INACTIVE))
        .title("🎯 Decision");
    
    let paragraph = Paragraph::new(content)
        .block(block)
        .wrap(Wrap { trim: true });
    
    f.render_widget(paragraph, area);
}

/// Helper: Wrap text to fit width
fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();
    
    for word in text.split_whitespace() {
        if current_line.len() + word.len() + 1 > max_width {
            if !current_line.is_empty() {
                lines.push(current_line);
                current_line = String::new();
            }
        }
        if !current_line.is_empty() {
            current_line.push(' ');
        }
        current_line.push_str(word);
    }
    
    if !current_line.is_empty() {
        lines.push(current_line);
    }
    
    lines
}

/// Activity Log: Scrolling list of recent events
fn render_activity_log(f: &mut Frame, area: Rect, state: &State) {
    let items: Vec<ListItem> = state.activity_log.iter().skip(state.scroll_offset).map(|log| {
        let (icon, color) = match log.level {
            LogLevel::Info => ("ℹ", COLOR_INFO),
            LogLevel::Warning => ("⚠", COLOR_WARNING),
            LogLevel::Error => ("✗", COLOR_DANGER),
            LogLevel::Success => ("✓", COLOR_SUCCESS),
        };
        
        let time_str = log.timestamp.format("%H:%M:%S").to_string();
        
        ListItem::new(Line::from(vec![
            Span::styled(format!("[{}] ", time_str), Style::default().fg(COLOR_MUTED)),
            Span::styled(format!("{} ", icon), Style::default().fg(color).add_modifier(Modifier::BOLD)),
            Span::styled(&log.message, Style::default().fg(COLOR_TEXT)),
        ]))
    }).collect();
    
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(COLOR_BORDER_INACTIVE))
                .title("📜 Live Audit (last 50 events)")
        );
    
    f.render_widget(list, area);
}

/// Footer: Keyboard legend and treasury address
fn render_footer(f: &mut Frame, area: Rect, state: &State) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(area);
    
    // Keyboard legend with grouping
    let legend = Line::from(vec![
        // Navigation
        Span::styled("↑↓", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
        Span::styled("/", Style::default().fg(COLOR_MUTED)),
        Span::styled("jk", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
        Span::styled(":Nav ", Style::default().fg(COLOR_MUTED)),
        
        Span::styled("│ ", Style::default().fg(COLOR_MUTED)),
        
        // Actions
        Span::styled("s", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
        Span::styled(":Scan ", Style::default().fg(COLOR_MUTED)),
        Span::styled("d", Style::default().fg(COLOR_INFO).add_modifier(Modifier::BOLD)),
        Span::styled(":Dry ", Style::default().fg(COLOR_MUTED)),
        Span::styled("e", Style::default().fg(COLOR_DANGER).add_modifier(Modifier::BOLD)),
        Span::styled(":Exec ", Style::default().fg(COLOR_MUTED)),
        
        Span::styled("│ ", Style::default().fg(COLOR_MUTED)),
        
        // Tools
        Span::styled("x", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
        Span::styled(":Export ", Style::default().fg(COLOR_MUTED)),
        Span::styled("w", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
        Span::styled(":Whitelist ", Style::default().fg(COLOR_MUTED)),
        
        Span::styled("│ ", Style::default().fg(COLOR_MUTED)),
        
        // Help & Quit
        Span::styled("?", Style::default().fg(COLOR_INFO).add_modifier(Modifier::BOLD)),
        Span::styled(":Help ", Style::default().fg(COLOR_MUTED)),
        Span::styled("q", Style::default().fg(COLOR_DANGER).add_modifier(Modifier::BOLD)),
        Span::styled(":Quit", Style::default().fg(COLOR_MUTED)),
    ]);
    
    let legend_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_BORDER_INACTIVE));
    let legend_para = Paragraph::new(legend).block(legend_block);
    f.render_widget(legend_para, chunks[0]);
    
    // Treasury address with visual emphasis
    let treasury_short = if state.treasury_address.len() >= 14 {
        format!("{}...{}", &state.treasury_address[..6], &state.treasury_address[state.treasury_address.len()-6..])
    } else {
        state.treasury_address.clone()
    };
    
    let treasury = Line::from(vec![
        Span::styled("💰 ", Style::default().fg(COLOR_WARNING)),
        Span::styled(treasury_short, Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
    ]);
    
    let treasury_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_SUCCESS));
    let treasury_para = Paragraph::new(treasury).block(treasury_block).alignment(Alignment::Center);
    f.render_widget(treasury_para, chunks[1]);
}

/// Help Overlay: Interactive guide
fn render_help_overlay(f: &mut Frame, size: Rect) {
    // Center the help box
    let area = centered_rect(80, 90, size);
    
    // Clear background
    f.render_widget(Clear, area);
    
    let help_text = vec![
        Line::from(Span::styled("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━", Style::default().fg(COLOR_PRIMARY))),
        Line::from(Span::styled("                    ⚡ KORA NEXUS HELP                    ", Style::default().fg(COLOR_PRIMARY).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━", Style::default().fg(COLOR_PRIMARY))),
        Line::from(""),
        
        Line::from(Span::styled("NAVIGATION", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD | Modifier::UNDERLINED))),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ↑ / k       ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
            Span::styled("Move selection up", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  ↓ / j       ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
            Span::styled("Move selection down", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  PgUp/PgDn   ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
            Span::styled("Jump 10 rows", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  Mouse Scroll", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
            Span::styled("Navigate with mouse wheel", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  E           ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
            Span::styled("Expand address details", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(""),
        
        Line::from(Span::styled("CORE OPERATIONS", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD | Modifier::UNDERLINED))),
        Line::from(""),
        Line::from(vec![
            Span::styled("  s           ", Style::default().fg(COLOR_SUCCESS).add_modifier(Modifier::BOLD)),
            Span::styled("Scan for sponsored accounts", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  d           ", Style::default().fg(COLOR_INFO).add_modifier(Modifier::BOLD)),
            Span::styled("Dry-run selected account (preview)", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  e           ", Style::default().fg(COLOR_DANGER).add_modifier(Modifier::BOLD)),
            Span::styled("Execute reclaim (", Style::default().fg(COLOR_TEXT)),
            Span::styled("requires dry-run first", Style::default().fg(COLOR_WARNING)),
            Span::styled(")", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  x           ", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
            Span::styled("Export to CSV (timestamped report)", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  w           ", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD)),
            Span::styled("Whitelist account (protect from reclaim)", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(""),
        
        Line::from(Span::styled("UTILITY", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD | Modifier::UNDERLINED))),
        Line::from(""),
        Line::from(vec![
            Span::styled("  r           ", Style::default().fg(COLOR_INFO).add_modifier(Modifier::BOLD)),
            Span::styled("Refresh stats from database", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  m           ", Style::default().fg(COLOR_INFO).add_modifier(Modifier::BOLD)),
            Span::styled("Toggle mode (Monitor/DryRun/Execute)", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  ?           ", Style::default().fg(COLOR_PRIMARY).add_modifier(Modifier::BOLD)),
            Span::styled("Show this help (press again to close)", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  q / ESC     ", Style::default().fg(COLOR_DANGER).add_modifier(Modifier::BOLD)),
            Span::styled("Quit application", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(""),
        
        Line::from(Span::styled("SAFETY RULES", Style::default().fg(COLOR_DANGER).add_modifier(Modifier::BOLD | Modifier::UNDERLINED))),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ⚠ ", Style::default().fg(COLOR_WARNING)),
            Span::styled("Dry-run REQUIRED before execute", Style::default().fg(COLOR_TEXT).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  🔒 ", Style::default().fg(COLOR_DANGER)),
            Span::styled("Whitelisted accounts CANNOT be reclaimed", Style::default().fg(COLOR_TEXT).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::styled("  ✓ ", Style::default().fg(COLOR_SUCCESS)),
            Span::styled("Execute sends real on-chain transaction", Style::default().fg(COLOR_TEXT).add_modifier(Modifier::BOLD)),
        ]),
        Line::from(""),
        
        Line::from(Span::styled("STATUS INDICATORS", Style::default().fg(COLOR_WARNING).add_modifier(Modifier::BOLD | Modifier::UNDERLINED))),
        Line::from(""),
        Line::from(vec![
            Span::styled("  ✓ ELIGIBLE      ", Style::default().fg(COLOR_SUCCESS)),
            Span::styled("Ready to reclaim (after dry-run)", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  🔒 WHITELISTED  ", Style::default().fg(COLOR_MUTED)),
            Span::styled("Protected account", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  ⟳ PROCESSING   ", Style::default().fg(COLOR_INFO)),
            Span::styled("Transaction in progress", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  ✓ RECLAIMED    ", Style::default().fg(COLOR_PRIMARY)),
            Span::styled("Successfully reclaimed", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  ✗ FAILED       ", Style::default().fg(COLOR_DANGER)),
            Span::styled("Operation failed", Style::default().fg(COLOR_TEXT)),
        ]),
        Line::from(""),
        
        Line::from(Span::styled("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━", Style::default().fg(COLOR_PRIMARY))),
        Line::from(vec![
            Span::styled("Press ", Style::default().fg(COLOR_MUTED)),
            Span::styled("?", Style::default().fg(COLOR_INFO).add_modifier(Modifier::BOLD)),
            Span::styled(" or ", Style::default().fg(COLOR_MUTED)),
            Span::styled("ESC", Style::default().fg(COLOR_INFO).add_modifier(Modifier::BOLD)),
            Span::styled(" to close this help", Style::default().fg(COLOR_MUTED)),
        ]),
        Line::from(Span::styled("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━", Style::default().fg(COLOR_PRIMARY))),
    ];
    
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(COLOR_PRIMARY).add_modifier(Modifier::BOLD))
        .style(Style::default().bg(Color::Black));
    
    let paragraph = Paragraph::new(help_text)
        .block(block)
        .wrap(Wrap { trim: false });
    
    f.render_widget(paragraph, area);
}

/// Helper: Create a centered rectangle
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
