use ratatui::prelude::*;
use ratatui::symbols::Marker;
use ratatui::widgets::{Axis, Chart, Dataset, GraphType, Paragraph};

use crate::app::App;
use crate::units::{fmt_bytes, fmt_rate};
use super::theme;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let block = theme::panel_block("Network", false);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(fast) = app.fast.as_ref() else {
        f.render_widget(Paragraph::new("n/a").style(Style::default().fg(theme::DIM)), inner);
        return;
    };

    let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(2)]).split(inner);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!("▼ {}", fmt_rate(fast.net_rx_rate)), Style::default().fg(theme::ACCENT)),
            Span::styled(
                format!("  ▲ {}", fmt_rate(fast.net_tx_rate)),
                Style::default().fg(theme::gradient(0.55)),
            ),
            Span::styled(
                format!("  ∑ ▼{} ▲{}", fmt_bytes(fast.net_rx_total), fmt_bytes(fast.net_tx_total)),
                Style::default().fg(theme::DIM),
            ),
        ])),
        rows[0],
    );

    let down: Vec<(f64, f64)> =
        app.net_hist.iter().enumerate().map(|(i, v)| (i as f64, v.0)).collect();
    let up: Vec<(f64, f64)> =
        app.net_hist.iter().enumerate().map(|(i, v)| (i as f64, -v.1)).collect();
    let peak = down
        .iter()
        .map(|p| p.1)
        .chain(up.iter().map(|p| -p.1))
        .fold(1024.0, f64::max) // ≥1 KB/s so an idle machine doesn't render noise
        * 1.2;

    let chart = Chart::new(vec![
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(theme::ACCENT))
            .data(&down),
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(theme::gradient(0.55)))
            .data(&up),
    ])
    .x_axis(Axis::default().bounds([0.0, 59.0]))
    .y_axis(Axis::default().bounds([-peak, peak]));
    f.render_widget(chart, rows[1]);
}
