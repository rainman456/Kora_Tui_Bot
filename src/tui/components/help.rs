use ratatui::{
    backend::Backend,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

pub fn render<B: Backend>(frame: &mut Frame<B>, area: Rect) {
    let keybindings = vec![
        ("Tab / Shift+Tab", "Next / Previous screen"),
        ("↑ / ↓", "Navigate items"),
        ("Enter", "Select / Confirm"),
        ("r", "Refresh data"),
        ("s", "Scan for eligible accounts"),
        ("c", "Reclaim selected account"),
        ("h / ?", "Toggle help"),
        ("q / Esc", "Quit"),
    ];
    
    let items: Vec<ListItem> = keybindings
        .iter()
        .map(|(key, desc)| {
            let content = vec![
                Span::styled(
                    format!("{:15}", key),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" - "),
                Span::styled(*desc, Style::default().fg(Color::White)),
            ];
            ListItem::new(Line::from(content))
        })
        .collect();
    
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Keybindings")
                .border_style(Style::default().fg(Color::Cyan)),
        );
    
    frame.render_widget(list, area);
}