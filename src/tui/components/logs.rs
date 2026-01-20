use ratatui::{
    backend::Backend,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};
use crate::tui::app::{App, LogLevel};

pub fn render<B: Backend>(frame: &mut Frame<B>, area: Rect, app: &App) {
    let items: Vec<ListItem> = app
        .logs
        .iter()
        .rev()
        .take(area.height as usize - 2)
        .map(|log| {
            let (icon, color) = match log.level {
                LogLevel::Info => ("ℹ", Color::Blue),
                LogLevel::Success => ("✓", Color::Green),
                LogLevel::Warning => ("⚠", Color::Yellow),
                LogLevel::Error => ("✗", Color::Red),
            };
            
            let content = vec![
                Span::styled(
                    format!("{} ", icon),
                    Style::default().fg(color),
                ),
                Span::styled(
                    log.timestamp.format("[%H:%M:%S]").to_string(),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::raw(" "),
                Span::raw(&log.message),
            ];
            
            ListItem::new(Line::from(content))
        })
        .collect();
    
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Logs")
                .border_style(Style::default().fg(Color::Cyan)),
        );
    
    frame.render_widget(list, area);
}