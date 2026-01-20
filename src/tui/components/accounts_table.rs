use ratatui::{
    backend::Backend,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table, TableState},
    Frame,
};
use crate::{
    tui::app::App,
    storage::models::AccountStatus,
};

pub fn render<B: Backend>(frame: &mut Frame<B>, area: Rect, app: &mut App) {
    let header_cells = ["Pubkey", "Created", "Status", "Rent (SOL)", "Data Size"]
        .iter()
        .map(|h| Cell::from(*h).style(Style::default().fg(Color::Yellow)));
    
    let header = Row::new(header_cells)
        .style(Style::default().add_modifier(Modifier::BOLD))
        .height(1)
        .bottom_margin(1);
    
    let rows = app.accounts.iter().map(|account| {
        let status_color = match account.status {
            AccountStatus::Active => Color::Green,
            AccountStatus::Closed => Color::Yellow,
            AccountStatus::Reclaimed => Color::Gray,
        };
        
        let cells = vec![
            Cell::from(truncate_pubkey(&account.pubkey)),
            Cell::from(account.created_at.format("%Y-%m-%d").to_string()),
            Cell::from(format!("{:?}", account.status)).style(Style::default().fg(status_color)),
            Cell::from(format!("{:.4}", account.rent_lamports as f64 / 1_000_000_000.0)),
            Cell::from(account.data_size.to_string()),
        ];
        
        Row::new(cells).height(1)
    });
    
    let table = Table::new(rows)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Accounts")
                .border_style(Style::default().fg(Color::Cyan)),
        )
        .widths(&[
            ratatui::layout::Constraint::Percentage(30),
            ratatui::layout::Constraint::Percentage(20),
            ratatui::layout::Constraint::Percentage(15),
            ratatui::layout::Constraint::Percentage(20),
            ratatui::layout::Constraint::Percentage(15),
        ])
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");
    
    let mut state = TableState::default();
    state.select(Some(app.selected_account_index));
    
    frame.render_stateful_widget(table, area, &mut state);
}

fn truncate_pubkey(pubkey: &str) -> String {
    if pubkey.len() > 20 {
        format!("{}...{}", &pubkey[..8], &pubkey[pubkey.len()-8..])
    } else {
        pubkey.to_string()
    }
}