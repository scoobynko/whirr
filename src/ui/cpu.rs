use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Wrap};

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
        // total_cpu is documented 0..100, so "100%" (the widest case) never
        // gets close to overflowing a 28-wide card, but big_text_fit guards
        // it the same way temp.rs/power.rs guard their hero numbers — a
        // dropped '%' is a narrower, still-legible fallback if that ever
        // changes.
        let precise = format!("{:.0}%", fast.total_cpu);
        let coarse = format!("{:.0}", fast.total_cpu);
        let hero = font::hero_lines(&precise, &coarse, inner.width, theme::ACCENT);
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
    let data: Vec<u64> = app.cpu_hist.iter().map(|v| v.round() as u64).collect();
    super::spark::render(f, area, &data, 100, Style::default().fg(theme::ACCENT));
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
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

    fn draw_buffer(w: u16, h: u16) -> Buffer {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        let mut app = App::demo();
        app.statics.e_cores = 2;
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        t.backend().buffer().clone()
    }

    #[test]
    fn hero_with_strip_when_room() {
        // demo total_cpu = 41.0 → "41%". The hero digits used to be drawn
        // with foreground quadrant glyphs, which seam in Terminal.app — see
        // `ui/font.rs`'s doc comment. They're background-filled cells now,
        // so prove it by sampling the buffer's bg colours directly rather
        // than looking for glyph characters in the rendered text.
        let full = draw(40, 12);
        let buf = draw_buffer(40, 12);
        let filled = buf.content().iter().filter(|c| c.style().bg == Some(super::theme::ACCENT)).count();
        let expected: usize =
            crate::ui::font::big_text("41%").iter().flat_map(|r| r.chars()).filter(|&c| c == '#').count();
        assert_eq!(filled, expected, "hero bitmap pixel count mismatch for \"41%\"");
        assert!(full.contains(" P "), "per-core strip P label missing");
        assert!(!full.contains(" 12 "), "numbered heatmap cell should be gone in hero tier");
    }

    #[test]
    fn compact_keeps_numbered_heatmap() {
        let compact = draw(40, 10);
        assert!(compact.contains(" 12"), "per-core numbered cell missing"); // demo core 0 at 12%
        let buf = draw_buffer(40, 10);
        let filled = buf.content().iter().any(|c| c.style().bg == Some(super::theme::ACCENT));
        assert!(!filled, "compact tier must not paint any hero bitmap pixels");
    }

    #[test]
    fn history_renders_block_sparkline() {
        // Drive the chart helper directly with an area exactly as wide as
        // the pushed history, so the whole buffer IS the chart: every column
        // is pinned to a known sample, not just "a bar appears somewhere".
        let mut t = Terminal::new(TestBackend::new(5, 1)).unwrap();
        let mut app = App::new(false);
        for v in [5.0_f32, 15.0, 30.0, 55.0, 95.0] {
            app.cpu_hist.push(v);
        }
        t.draw(|f| super::render_history(f, f.area(), &app)).unwrap();
        let s: String = t.backend().buffer().content().iter().map(|c| c.symbol()).collect();
        // levels = round(value) * 8 / 100: 5→0(' '), 15→1(▁), 30→2(▂), 55→4(▄), 95→7(▇)
        assert_eq!(s, " ▁▂▄▇", "chart mis-scaled or mis-positioned");
        assert_eq!(
            s.chars().last().unwrap(),
            '▇',
            "newest sample (95, the tallest) must land at the right edge"
        );
    }
}
