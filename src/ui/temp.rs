use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

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
        let precise = format!("{t:.1}°C");
        let coarse = format!("{t:.0}°C");
        let hero: Vec<Line> = font::big_text_fit(&precise, &coarse, inner.width)
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
    // Baseline-shift 30→105 °C onto 0→75 so the idle-to-hot band uses the full
    // bar height instead of hugging the top.
    let data: Vec<u64> = app
        .temp_hist
        .iter()
        .map(|v| (v - 30.0).clamp(0.0, 75.0).round() as u64)
        .collect();
    super::spark::render(f, area, &data, 75, Style::default().fg(color));
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

    #[test]
    fn hero_falls_back_to_coarse_when_precise_would_overflow() {
        // 30x12 -> inner 28x10, full hero tier engaged (width >= 28, height >= 9).
        // "100.5°C" formatted to 1 decimal is 31 glyph-columns wide, wider than
        // the 28-wide inner area, so it must fall back to "100°C" (0 decimals)
        // instead of truncating mid-glyph. The precise-width row never fits
        // inside 28 cols, so its top row is not a substring of the buffer;
        // the coarse row (23 cols) does fit and must appear intact.
        let mut t = Terminal::new(TestBackend::new(30, 12)).unwrap();
        let mut app = App::demo();
        app.medium.as_mut().unwrap().temp_c = Some(100.5);
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        let content: String =
            t.backend().buffer().content().iter().map(|c| c.symbol()).collect();
        assert!(!content.contains('?'), "hero glyph fallback '?' should never render");
        let precise_row0 = super::font::big_text("100.5°C").remove(0);
        let coarse_row0 = super::font::big_text("100°C").remove(0);
        assert!(
            !content.contains(precise_row0.as_str()),
            "full-precision hero row rendered intact — should have overflowed the 28-wide card"
        );
        assert!(
            content.contains(coarse_row0.as_str()),
            "coarse fallback row missing — hero should fall back to \"100°C\" when precise overflows"
        );
    }

    #[test]
    fn history_renders_block_sparkline() {
        let mut t = Terminal::new(TestBackend::new(40, 12)).unwrap();
        let mut app = App::demo();
        for v in [40.0_f32, 60.0, 95.0, 70.0, 50.0] {
            app.temp_hist.push(v);
        }
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        let s: String = t.backend().buffer().content().iter().map(|c| c.symbol()).collect();
        assert!(s.contains('█') || s.contains('▇'), "temp history should render filled block bars");
    }
}
