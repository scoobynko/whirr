use ratatui::prelude::*;
use ratatui::symbols::Marker;
use ratatui::widgets::{Axis, Chart, Dataset, GraphType, Paragraph};

use crate::app::App;
use super::{font, theme};

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let block = theme::panel_block("Temp", false);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let temp = app.medium.as_ref().and_then(|m| m.temp_c);
    let Some(t) = temp else {
        f.render_widget(Paragraph::new("n/a").style(Style::default().fg(theme::DIM)), inner);
        return;
    };
    let color = theme::temp_color(t);

    if font::hero_fits(inner) {
        let rows = Layout::vertical([Constraint::Length(4), Constraint::Min(3)]).split(inner);
        let hero: Vec<Line> = font::big_text(&format!("{t:.1}°C"))
            .into_iter()
            .map(|r| Line::styled(r, Style::default().fg(color)))
            .collect();
        f.render_widget(Paragraph::new(hero), rows[0]);
        render_chart(f, rows[1], app, color);
    } else {
        let cols = Layout::horizontal([Constraint::Length(3), Constraint::Min(4)]).split(inner);
        render_thermometer(f, cols[0], t, color);

        let rows = Layout::vertical([Constraint::Length(1), Constraint::Min(2)]).split(cols[1]);
        f.render_widget(
            Paragraph::new(Span::styled(format!("{t:.1}°C"), Style::default().fg(color).bold())),
            rows[0],
        );
        render_chart(f, rows[1], app, color);
    }
}

fn render_thermometer(f: &mut Frame, area: Rect, t: f32, color: Color) {
    let h = area.height as usize;
    if h < 2 {
        return;
    }
    let fill_ratio = ((t - 30.0) / 75.0).clamp(0.0, 1.0);
    let filled = ((h - 1) as f32 * fill_ratio).round() as usize;
    let mut lines = Vec::with_capacity(h);
    for row in 0..h - 1 {
        let from_bottom = h - 1 - row;
        let ch = if from_bottom <= filled { "▐█▌" } else { "▐ ▌" };
        lines.push(Line::styled(ch, Style::default().fg(color)));
    }
    lines.push(Line::styled(" ● ", Style::default().fg(color)));
    f.render_widget(Paragraph::new(lines), area);
}

fn render_chart(f: &mut Frame, area: Rect, app: &App, color: Color) {
    let points: Vec<(f64, f64)> = app
        .temp_hist
        .iter()
        .enumerate()
        .map(|(i, v)| (i as f64, f64::from(v)))
        .collect();
    let chart = Chart::new(vec![Dataset::default()
        .marker(Marker::Braille)
        .graph_type(GraphType::Line)
        .style(Style::default().fg(color))
        .data(&points)])
    .x_axis(Axis::default().bounds([0.0, 59.0]))
    .y_axis(Axis::default().bounds([30.0, 105.0]));
    f.render_widget(chart, area);
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::app::App;

    fn draw(w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        let app = App::demo();
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        t.backend().buffer().content().iter().map(|c| c.symbol()).collect()
    }

    #[test]
    fn hero_drops_thermometer_when_room() {
        // demo temp = 88.0 → "88.0°C"
        let full = draw(40, 12);
        assert!(full.contains("▄▀▀▄"), "4-row '8' glyph missing");
        assert!(!full.contains("▐"), "thermometer should be gone in hero tier");
        let compact = draw(40, 10);
        assert!(compact.contains("▐"), "thermometer missing in compact tier");
        assert!(compact.contains("88.0°C"));
    }

    #[test]
    fn fill_ratio_clamps() {
        let ratio = |t: f32| ((t - 30.0) / 75.0).clamp(0.0, 1.0);
        assert_eq!(ratio(20.0), 0.0);
        assert_eq!(ratio(105.0), 1.0);
        assert!((ratio(67.5) - 0.5).abs() < 0.01);
    }
}
