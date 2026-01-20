use ratatui::{
    backend::Backend,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table},
    Frame,
};
use crate::tui::app::App;

pub fn render<B: Backend>(frame: &mut Frame<B>, area: Rect, app: &App) {
    let header_cells = ["Timestamp", "Account", "Amount (SOL)", "Signature", "Status"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(Color::Yellow)));
    
    let header = Row::new(header_cells)
        .style(Style::default().add_modifier(Modifier::BOLD))
        .height(1)
        .bottom_margin(1);
    
    let rows = app.operations.iter().map(|op| {
        let cells = vec![
            Cell::from(op.timestamp.format("%Y-%m-%d %H:%M").to_string()),
            Cell::from(truncate_address(&op.account)),
            Cell::from(format!("{:.4}", op.amount as f64 / 1_000_000_000.0)),
            Cell::from(truncate_address(&op.signature)),
            Cell::from(op.status.clone()).style(Style::default().fg(Color::Green)),
        ];
        
        Row::new(cells).height(1)
    });
    
    let table = Table::new(rows)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Reclaim Operations History")
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .widths(&[
            ratatui::layout::Constraint::Percentage(20),
            ratatui::layout::Constraint::Percentage(25),
            ratatui::layout::Constraint::Percentage(15),
            ratatui::layout::Constraint::Percentage(25),
            ratatui::layout::Constraint::Percentage(15),
        ]);
    
    frame.render_widget(table, area);
}

fn truncate_address(addr: &str) -> String {
    if addr.len() > 16 {
        format!("{}...{}", &addr[..6], &addr[addr.len()-6..])
    } else {
        addr.to_string()
    }
}