use ratatui::prelude::*;
use ratatui::symbols::Marker;
use ratatui::widgets::{Axis, Chart, Dataset, GraphType, Paragraph, Wrap};

use super::{font, theme};
use crate::app::App;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let block = theme::panel_block("CPU", false);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(fast) = app.fast.as_ref() else {
        f.render_widget(Paragraph::new("n/a").style(Style::default().fg(theme::DIM)), inner);
        return;
    };

    if font::hero_fits(inner) {
        let rows = Layout::vertical([
            Constraint::Length(4), // hero
            Constraint::Length(1), // per-core strip
            Constraint::Min(3),    // history chart
        ])
        .split(inner);
        let hero: Vec<Line> = font::big_text(&format!("{:.0}%", fast.total_cpu))
            .into_iter()
            .map(|r| Line::styled(r, Style::default().fg(theme::ACCENT)))
            .collect();
        f.render_widget(Paragraph::new(hero), rows[0]);
        render_core_strip(f, rows[1], app, &fast.per_core);
        render_history(f, rows[2], app);
    } else {
        let rows = Layout::vertical([Constraint::Length(3), Constraint::Min(3)]).split(inner);
        render_heatmap(f, rows[0], app, &fast.per_core);
        render_history(f, rows[1], app);
        // compact tier keeps the small current-% label over the chart
        let label = Line::from(Span::styled(
            format!("{:>3.0}%", fast.total_cpu),
            Style::default().fg(theme::ACCENT).bold(),
        ))
        .right_aligned();
        f.render_widget(Paragraph::new(label), Rect { height: 1, ..rows[1] });
    }
}

/// One colored cell per core — load carried by color alone (heat strip).
fn render_core_strip(f: &mut Frame, area: Rect, app: &App, per_core: &[f32]) {
    let e = app.statics.e_cores.min(per_core.len());
    let mut spans = vec![Span::styled("E ", Style::default().fg(theme::DIM))];
    for (i, &load) in per_core.iter().enumerate() {
        if i == e {
            spans.push(Span::styled("  P ", Style::default().fg(theme::DIM)));
        }
        spans.push(Span::styled("█", Style::default().fg(theme::gradient(load / 100.0))));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
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
    // On machines with many cores (e.g. M5 P-cores) the cell spans overflow a
    // single line's width; wrap onto the 3-row area allotted to the heatmap
    // instead of silently truncating the trailing cores off the panel edge.
    f.render_widget(
        Paragraph::new(Line::from(spans)).wrap(Wrap { trim: false }),
        area,
    );
}

fn render_history(f: &mut Frame, area: Rect, app: &App) {
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
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::app::App;

    fn draw(w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        let mut app = App::demo();
        // Pin the E/P boundary so the strip's labels don't depend on the
        // host machine's real core counts.
        app.statics.e_cores = 2;
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        t.backend().buffer().content().iter().map(|c| c.symbol()).collect()
    }

    #[test]
    fn hero_with_strip_when_room() {
        // demo total_cpu = 41.0 → "41%"
        let full = draw(40, 12);
        assert!(full.contains("▄  █"), "4-row '4' glyph missing");
        assert!(full.contains(" P "), "per-core strip P label missing");
        assert!(!full.contains(" 12 "), "numbered heatmap cell should be gone in hero tier");
    }

    #[test]
    fn compact_keeps_numbered_heatmap() {
        let compact = draw(40, 10);
        assert!(compact.contains(" 12"), "per-core numbered cell missing"); // demo core 0 at 12%
        assert!(!compact.contains("▄  █"));
    }
}
