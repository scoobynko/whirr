use ratatui::prelude::*;
use ratatui::symbols::Marker;
use ratatui::widgets::{Axis, Chart, Dataset, GraphType, Paragraph};

use crate::app::App;
use super::{font, theme};

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let block = theme::panel_block("Power", false);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(m) = app.medium.as_ref() else {
        f.render_widget(Paragraph::new("n/a").style(Style::default().fg(theme::DIM)), inner);
        return;
    };

    let rows = Layout::vertical([
        Constraint::Length(3), // hero number
        Constraint::Min(2),    // stacked chart
        Constraint::Length(1), // battery footer
    ])
    .split(inner);

    match &m.power {
        Some(p) => {
            let total = p.cpu_w + p.gpu_w + p.ane_w;
            let hero: Vec<Line> = font::big_text(&format!("{total:.1} W"))
                .into_iter()
                .map(|r| Line::styled(r, Style::default().fg(theme::ACCENT)))
                .collect();
            f.render_widget(Paragraph::new(hero), rows[0]);
            render_stack(f, rows[1], app);
        }
        None => {
            f.render_widget(Paragraph::new("n/a").style(Style::default().fg(theme::DIM)), rows[0]);
        }
    }

    let battery_line = match &m.battery {
        Some(b) => {
            let state = if b.charging { "⚡" } else { "🔋" };
            let health = b.health_pct.map_or(String::new(), |h| format!(" · health {h}%"));
            format!("{state} {}% · {} cycles{health}", b.percent, b.cycles)
        }
        None => String::new(), // desktop Mac: hide line
    };
    f.render_widget(
        Paragraph::new(battery_line).style(Style::default().fg(theme::DIM)),
        rows[2],
    );
}

/// Stacked braille lines: cpu, cpu+gpu, cpu+gpu+ane — three cumulative series.
fn render_stack(f: &mut Frame, area: Rect, app: &App) {
    let hist: Vec<(f64, f64, f64)> = app.power_hist.iter().collect();
    if hist.is_empty() {
        return;
    }
    let cpu: Vec<(f64, f64)> = hist.iter().enumerate().map(|(i, v)| (i as f64, v.0)).collect();
    let cpu_gpu: Vec<(f64, f64)> =
        hist.iter().enumerate().map(|(i, v)| (i as f64, v.0 + v.1)).collect();
    let total: Vec<(f64, f64)> =
        hist.iter().enumerate().map(|(i, v)| (i as f64, v.0 + v.1 + v.2)).collect();
    let y_max = total.iter().map(|p| p.1).fold(1.0, f64::max) * 1.2;

    // total (brightest) drawn last so it sits on top
    let chart = Chart::new(vec![
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(theme::gradient(0.4)))
            .data(&cpu),
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(theme::gradient(0.7)))
            .data(&cpu_gpu),
        Dataset::default()
            .marker(Marker::Braille)
            .graph_type(GraphType::Line)
            .style(Style::default().fg(theme::ACCENT))
            .data(&total),
    ])
    .x_axis(Axis::default().bounds([0.0, 59.0]))
    .y_axis(Axis::default().bounds([0.0, y_max]));
    f.render_widget(chart, area);
}
