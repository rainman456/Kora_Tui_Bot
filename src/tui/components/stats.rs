use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use crate::tui::app::App;

pub fn render<B: Backend>(frame: &mut Frame<B>, area: Rect, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(area);
    
    // Total Accounts
    render_stat(
        frame,
        chunks[0],
        "Total Accounts",
        app.stats.total_accounts.to_string(),
        Color::Cyan,
    );
    
    // Eligible for Reclaim
    render_stat(
        frame,
        chunks[1],
        "Eligible",
        app.stats.eligible_accounts.to_string(),
        Color::Green,
    );
    
    // Total Rent Locked
    render_stat(
        frame,
        chunks[2],
        "Rent Locked",
        format!("{:.4} SOL", app.stats.total_rent_locked as f64 / 1_000_000_000.0),
        Color::Yellow,
    );
    
    // Total Reclaimed
    render_stat(
        frame,
        chunks[3],
        "Reclaimed",
        format!("{:.4} SOL", app.stats.total_rent_reclaimed as f64 / 1_000_000_000.0),
        Color::Green,
    );
}

fn render_stat<B: Backend>(
    frame: &mut Frame<B>,
    area: Rect,
    label: &str,
    value: String,
    color: Color,
) {
    let text = vec![
        Line::from(Span::styled(
            label,
            Style::default().fg(Color::Gray),
        )),
        Line::from(Span::styled(
            value,
            Style::default()
                .fg(color)
                .add_modifier(Modifier::BOLD),
        )),
    ];
    
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    
    let paragraph = Paragraph::new(text)
        .block(block)
        .alignment(ratatui::layout::Alignment::Center);
    
    frame.render_widget(paragraph, area);
}