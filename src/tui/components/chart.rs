use ratatui::{
    backend::Backend,
    layout::Rect,
    style::{Color, Style},
    symbols,
    text::Span,
    widgets::{Axis, Block, Borders, Chart, Dataset, GraphType},
    Frame,
};
use crate::tui::app::App;

pub fn render<B: Backend>(frame: &mut Frame<B>, area: Rect, app: &App) {
    // Sample data - in real implementation, load from operations history
    let data: Vec<(f64, f64)> = (0..30)
        .map(|i| {
            let x = i as f64;
            let y = (i as f64 * 0.1).sin() * 50.0 + 50.0;
            (x, y)
        })
        .collect();
    
    let datasets = vec![Dataset::default()
        .name("Rent Reclaimed (SOL)")
        .marker(symbols::Marker::Braille)
        .style(Style::default().fg(Color::Cyan))
        .graph_type(GraphType::Line)
        .data(&data)];
    
    let chart = Chart::new(datasets)
        .block(
            Block::default()
                .title(Span::styled(
                    "Reclaim Activity (Last 30 Days)",
                    Style::default().fg(Color::Cyan),
                ))
                .borders(Borders::ALL),
        )
        .x_axis(
            Axis::default()
                .title("Days")
                .style(Style::default().fg(Color::Gray))
                .bounds([0.0, 30.0]),
        )
        .y_axis(
            Axis::default()
                .title("SOL")
                .style(Style::default().fg(Color::Gray))
                .bounds([0.0, 100.0])
                .labels(vec![
                    Span::raw("0"),
                    Span::raw("50"),
                    Span::raw("100"),
                ]),
        );
    
    frame.render_widget(chart, area);
}