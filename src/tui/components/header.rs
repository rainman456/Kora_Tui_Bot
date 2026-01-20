use ratatui::{
    backend::Backend,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};
use crate::tui::app::App;

pub fn render<B: Backend>(frame: &mut Frame<B>, area: Rect, app: &App) {
    let title = vec![
        Span::styled(
            "⚡ ",
            Style::default().fg(Color::Yellow),
        ),
        Span::styled(
            "Kora Rent Reclaim Bot",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" | "),
        Span::styled(
            format!("Network: {:?}", app.config.solana.network),
            Style::default().fg(Color::Green),
        ),
    ];
    
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
    
    let paragraph = Paragraph::new(Line::from(title))
        .block(block)
        .alignment(Alignment::Center);
    
    frame.render_widget(paragraph, area);
}