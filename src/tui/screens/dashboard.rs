use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};
use crate::tui::{app::App, components};

pub fn render<B: Backend>(frame: &mut Frame<B>, area: Rect, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),   // Stats
            Constraint::Percentage(40), // Chart
            Constraint::Percentage(30), // Accounts preview
            Constraint::Min(0),      // Logs
        ])
        .split(area);
    
    // Render statistics
    components::stats::render(frame, chunks[0], app);
    
    // Render chart
    components::chart::render(frame, chunks[1], app);
    
    // Render accounts table (preview)
    components::accounts_table::render(frame, chunks[2], app);
    
    // Render logs
    components::logs::render(frame, chunks[3], app);
}