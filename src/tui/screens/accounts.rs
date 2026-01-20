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
            Constraint::Percentage(70), // Accounts table
            Constraint::Percentage(30), // Account details/help
        ])
        .split(area);
    
    // Render full accounts table
    components::accounts_table::render(frame, chunks[0], app);
    
    // Render help or selected account details
    components::help::render(frame, chunks[1]);
}