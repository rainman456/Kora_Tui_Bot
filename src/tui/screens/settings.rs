use ratatui::{
    backend::Backend,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};
use crate::tui::app::App;

pub fn render<B: Backend>(frame: &mut Frame<B>, area: Rect, app: &App) {
    let settings = vec![
        ("RPC URL", app.config.solana.rpc_url.clone()),
        ("Network", format!("{:?}", app.config.solana.network)),
        ("Operator Pubkey", truncate(&app.config.kora.operator_pubkey)),
        ("Treasury Wallet", truncate(&app.config.kora.treasury_wallet)),
        ("Min Inactive Days", app.config.reclaim.min_inactive_days.to_string()),
        ("Auto Reclaim", app.config.reclaim.auto_reclaim_enabled.to_string()),
        ("Batch Size", app.config.reclaim.batch_size.to_string()),
    ];
    
    let items: Vec<ListItem> = settings
        .iter()
        .map(|(key, value)| {
            let content = vec![
                Span::styled(
                    format!("{:20}", key),
                    Style::default().fg(Color::Yellow),
                ),
                Span::raw(": "),
                Span::styled(value, Style::default().fg(Color::White)),
            ];
            ListItem::new(Line::from(content))
        })
        .collect();
    
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Configuration")
                .border_style(Style::default().fg(Color::Cyan)),
        );
    
    frame.render_widget(list, area);
}

fn truncate(s: &str) -> String {
    if s.len() > 40 {
        format!("{}...{}", &s[..16], &s[s.len()-16..])
    } else {
        s.to_string()
    }
}