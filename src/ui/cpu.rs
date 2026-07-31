use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use super::{font, gauge, theme};
use crate::app::App;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let inner = gauge::frame(f, area, "CPU");
    let Some(fast) = app.fast.as_ref() else {
        return gauge::unavailable(f, inner);
    };

    if font::hero_fits(inner) {
        // The hero tier used to spend a row here on a per-core heat strip —
        // one colour-coded cell per core, split into E and P groups. It was
        // dropped: which individual core is loaded isn't actionable, the
        // hero total above answers "is the CPU busy", the Processes card
        // answers "what's making it busy", and the Power/Temp cards cover
        // "is the machine working hard" more legibly than teal shades can.
        // The row goes to the history chart instead, where it buys real
        // resolution.
        let rows = Layout::vertical([
            Constraint::Length(gauge::HERO_ROWS),
            Constraint::Min(3), // history chart
        ])
        .split(inner);
        // total_cpu is documented 0..100, so "100%" (the widest case) never
        // gets close to overflowing a 28-wide card, but the coarse fallback
        // guards it the same way temp.rs/power.rs guard their hero numbers —
        // a dropped '%' is a narrower, still-legible fallback if that ever
        // changes.
        let precise = format!("{:.0}%", fast.total_cpu);
        let coarse = format!("{:.0}", fast.total_cpu);
        gauge::hero(f, rows[0], &precise, &coarse, theme::ACCENT);
        render_history(f, rows[1], app);
    } else {
        // The compact tier used to spend its top 3 rows on a numbered per-core
        // heatmap. Dropped for the same reason as the hero tier's heat strip
        // (see above) — and it cost more here, on the tier with the least room
        // to spare. The chart takes the whole card, with the current % over it.
        render_history(f, inner, app);
        let label = Line::from(Span::styled(
            format!("{:>3.0}%", fast.total_cpu),
            Style::default().fg(theme::ACCENT).bold(),
        ))
        .right_aligned();
        f.render_widget(Paragraph::new(label), Rect { height: 1, ..inner });
    }
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
        // e_cores used to be pinned here so the per-core labels didn't depend
        // on the host's real core counts. Neither tier renders per-core data
        // any more, so nothing reads it.
        let app = App::demo();
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        t.backend().buffer().content().iter().map(|c| c.symbol()).collect()
    }

    fn draw_buffer(w: u16, h: u16) -> Buffer {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        let app = App::demo();
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        t.backend().buffer().clone()
    }

    #[test]
    fn hero_tier_is_hero_plus_chart_only() {
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
        assert!(!full.contains(" P "), "per-core heat strip should be gone from the hero tier");
    }

    /// The row reclaimed from the strip goes to the chart, so the sparkline
    /// now starts immediately under the 5-row hero instead of one row lower.
    /// Pinned by position rather than by "a bar appears somewhere", so a
    /// future layout change that quietly re-inserts a spacer row trips this.
    #[test]
    fn chart_starts_directly_under_the_hero() {
        // Push a full-scale sample so the newest column is guaranteed to
        // reach the chart's topmost row. Asserting on the demo fixture's own
        // history would only prove its peak (77 of 100) doesn't reach the
        // top — a property of the data, not of the layout.
        let mut app = App::demo();
        app.cpu_hist.push(100.0);
        let mut t = Terminal::new(TestBackend::new(40, 12)).unwrap();
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        let buf = t.backend().buffer().clone();

        // y=0 is the panel's top border, so the hero occupies y=1..=5 and the
        // chart's first row is y=6 — the row the strip used to own. The
        // newest sample lands at the right edge, just inside the border. A
        // completely full cell carries its colour as a background fill, not a
        // '█' glyph (see `ui/spark.rs`), so assert on the background.
        let x = buf.area.width - 2;
        assert_eq!(
            buf[(x, 6)].bg,
            super::theme::ACCENT,
            "a 100% sample should fill the reclaimed row y=6; chart did not grow into it"
        );
    }

    /// Per-core load is gone from both tiers, not just the hero one. The
    /// compact tier's version was the 3-row numbered heatmap (demo core 0 sat
    /// at 12%, rendered as a " 12" cell); the whole card is chart plus the
    /// current-% label now.
    #[test]
    fn compact_tier_is_chart_plus_percent_only() {
        let compact = draw(40, 10);
        assert!(!compact.contains(" 12"), "numbered per-core heatmap cell should be gone");
        assert!(!compact.contains(" P "), "per-core E/P labels should be gone");
        assert!(compact.contains("41%"), "compact tier should still label the current total");
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
