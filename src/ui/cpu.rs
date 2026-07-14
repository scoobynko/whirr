use ratatui::prelude::*;
use ratatui::symbols::Marker;
use ratatui::widgets::{Axis, Chart, Dataset, GraphType, Paragraph};

use super::theme;
use crate::app::App;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let block = theme::panel_block("CPU", false);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(fast) = app.fast.as_ref() else {
        f.render_widget(Paragraph::new("n/a").style(Style::default().fg(theme::DIM)), inner);
        return;
    };

    let rows = Layout::vertical([Constraint::Length(3), Constraint::Min(3)]).split(inner);
    render_heatmap(f, rows[0], app, &fast.per_core);
    render_history(f, rows[1], app, fast.total_cpu);
}

fn render_heatmap(f: &mut Frame, area: Rect, app: &App, per_core: &[f32]) {
    let e = app.statics.e_cores.min(per_core.len());
    let mut spans = vec![Span::styled("E ", Style::default().fg(theme::DIM))];
    for (i, &load) in per_core.iter().enumerate() {
        if i == e {
            spans.push(Span::styled("  P ", Style::default().fg(theme::DIM)));
        }
        spans.push(Span::styled(
            format!("{:>3}", (load as u16).min(99)),
            Style::default().fg(theme::TEXT).bg(theme::gradient(load / 100.0)),
        ));
        spans.push(Span::raw(" "));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_history(f: &mut Frame, area: Rect, app: &App, current: f32) {
    let points: Vec<(f64, f64)> = app
        .cpu_hist
        .iter()
        .enumerate()
        .map(|(i, v)| (i as f64, f64::from(v)))
        .collect();
    let dataset = Dataset::default()
        .marker(Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(theme::ACCENT))
        .data(&points);
    let chart = Chart::new(vec![dataset])
        .x_axis(Axis::default().bounds([0.0, 59.0]))
        .y_axis(Axis::default().bounds([0.0, 100.0]))
        .style(Style::default().fg(theme::DIM));
    f.render_widget(chart, area);
    let label = Line::from(Span::styled(
        format!("{current:>3.0}%"),
        Style::default().fg(theme::ACCENT).bold(),
    ))
    .right_aligned();
    f.render_widget(
        Paragraph::new(label),
        Rect { height: 1, ..area },
    );
}
