use ratatui::{
    backend::Backend,
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};
use crate::tui::{
    app::{App, Screen},
    components,
    screens,
};

pub fn render_ui<B: Backend>(frame: &mut Frame<B>, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(0),     // Content
            Constraint::Length(3),  // Status bar
        ])
        .split(frame.size());
    
    // Render header
    components::header::render(frame, chunks[0], app);
    
    // Render current screen
    match app.current_screen {
        Screen::Dashboard => screens::dashboard::render(frame, chunks[1], app),
        Screen::Accounts => screens::accounts::render(frame, chunks[1], app),
        Screen::Operations => screens::operations::render(frame, chunks[1], app),
        Screen::Settings => screens::settings::render(frame, chunks[1], app),
    }
    
    // Render status bar
    render_status_bar(frame, chunks[2], app);
}

fn render_status_bar<B: Backend>(frame: &mut Frame<B>, area: Rect, app: &App) {
    use ratatui::{
        style::{Color, Modifier, Style},
        text::{Line, Span},
        widgets::{Block, Borders, Paragraph},
    };
    
    let screen_indicator = match app.current_screen {
        Screen::Dashboard => "Dashboard",
        Screen::Accounts => "Accounts",
        Screen::Operations => "Operations",
        Screen::Settings => "Settings",
    };
    
    let status_text = if let Some(msg) = &app.status_message {
        msg.clone()
    } else {
        "Ready".to_string()
    };
    
    let text = Line::from(vec![
        Span::styled(
            format!(" {} ", screen_indicator),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            status_text,
            Style::default().fg(Color::Gray),
        ),
        Span::raw(" | "),
        Span::styled(
            "Tab: Next Screen",
            Style::default().fg(Color::Yellow),
        ),
        Span::raw(" | "),
        Span::styled(
            "q: Quit",
            Style::default().fg(Color::Red),
        ),
    ]);
    
    let paragraph = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL));
    
    frame.render_widget(paragraph, area);
}