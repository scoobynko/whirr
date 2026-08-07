use ratatui::prelude::*;

use crate::app::App;
use super::{font, gauge};

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let inner = gauge::frame(f, area, &app.theme, "Temp");
    let Some(t) = app.medium.as_ref().and_then(|m| m.temp_c) else {
        return gauge::unavailable(f, inner, &app.theme);
    };
    let color = app.theme.temp_color(t);

    // A 3-column vertical thermometer (`▐█▌` per row, a `●` bulb at the
    // bottom) used to sit to the left of the compact reading. Dropped: it
    // restated the number printed right beside it, it read as a stray progress
    // bar rather than a temperature, and stacked `▐█▌` rows seam in
    // Terminal.app the same way every other block-glyph column does (see
    // `ui/spark.rs`). The chart takes the full card width at both tiers now.
    let hero = font::hero_fits(inner);
    let head = if hero { gauge::HERO_ROWS } else { 1 };
    let rows = Layout::vertical([Constraint::Length(head), Constraint::Min(2)]).split(inner);
    if hero {
        gauge::hero(f, rows[0], &format!("{t:.1}°C"), &format!("{t:.0}°C"), color);
    } else {
        gauge::readout(f, rows[0], &format!("{t:.1}°C"), color);
    }
    render_chart(f, rows[1], app, color);
}

fn render_chart(f: &mut Frame, area: Rect, app: &App, color: Color) {
    // Baseline-shift 30→105 °C onto 0→75 so the idle-to-hot band uses the full
    // bar height instead of hugging the top.
    let data: Vec<u64> = app
        .temp_hist
        .iter()
        .map(|v| (v - 30.0).clamp(0.0, 75.0).round() as u64)
        .collect();
    super::spark::render(f, area, &app.theme, &data, 75, Style::default().fg(color));
}

#[cfg(test)]
mod tests {
    /// The palette the app starts with; tests assert against it by name
    /// rather than against raw RGB triples.
    const TH: crate::ui::theme::Theme = crate::ui::theme::Theme::dark();

    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::style::Color;
    use ratatui::Terminal;

    use crate::app::App;

    fn draw(w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        let app = App::demo();
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        t.backend().buffer().content().iter().map(|c| c.symbol()).collect()
    }

    fn draw_buffer(w: u16, h: u16, app: &App) -> Buffer {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| super::render(f, f.area(), app)).unwrap();
        t.backend().buffer().clone()
    }

    /// Reconstruct row `y` of the buffer as a `#`/space bitmap string by
    /// sampling each cell's background colour — the only place a filled hero
    /// pixel shows up now that hero digits are background fills rather than
    /// foreground glyph characters (see `ui/font.rs`'s doc comment).
    fn bg_bitmap_row(buf: &Buffer, y: u16, width: u16, color: Color) -> String {
        (0..width).map(|x| if buf[(x, y)].style().bg == Some(color) { '#' } else { ' ' }).collect()
    }

    #[test]
    fn neither_tier_draws_a_thermometer() {
        // demo temp = 88.0 → "88.0°C". The hero digits are background-filled
        // cells now, not foreground quadrant glyphs, so prove the "8" reads
        // correctly by sampling bg colours off the buffer and matching them
        // against the same bitmap `hero_lines` was built from, rather than
        // grepping the rendered text for glyph characters.
        let app = App::demo();
        let color = TH.temp_color(88.0);
        let buf = draw_buffer(40, 12, &app);
        // The card's border occupies row 0, so the hero rows start at y=1,
        // not y=0 — scan every row and require each expected bitmap row to
        // appear (as a substring, since it's left-padded by the border/x
        // offset) somewhere in the buffer, rather than hard-coding its exact
        // row/column position.
        let rows: Vec<String> = (0..buf.area.height).map(|y| bg_bitmap_row(&buf, y, 40, color)).collect();
        let expected = super::font::big_text("88.0°C");
        for want in &expected {
            assert!(
                rows.iter().any(|row| row.contains(want.as_str())),
                "no buffer row has bitmap {want:?} for \"88.0°C\""
            );
        }
        // The vertical `▐█▌` thermometer is gone from both tiers now, not
        // just the hero one — it restated the reading printed beside it and
        // seamed in Terminal.app. The compact tier keeps the numeric readout
        // and gives the rest of the card to the chart.
        let full = draw(40, 12);
        assert!(!full.contains("▐"), "thermometer should be gone in hero tier");
        let compact = draw(40, 10);
        assert!(!compact.contains("▐"), "thermometer should be gone in compact tier too");
        assert!(!compact.contains("●"), "thermometer bulb should be gone as well");
        assert!(compact.contains("88.0°C"), "compact tier should still print the reading");
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
        // The quadrant font is narrower than the old full-block face (3-col
        // digits instead of 4), so a realistic 3-integer-digit reading like
        // "100.5°C" (26 glyph-cols) now fits the 28-wide card and no longer
        // exercises the fallback — at this card-width floor, no realistic
        // sensor temperature (up to ~150°C) overflows any more. The guard
        // still has to hold for garbage/corrupted sensor input, so drive it
        // with a value no real Mac would report: "1000.4°C" formatted to 1
        // decimal is 30 glyph-columns wide, wider than the 28-wide inner
        // area, so it must fall back to "1000°C" (0 decimals) instead of
        // truncating mid-glyph. The precise-width row never fits inside 28
        // cols, so its top row is not a substring of the buffer; the coarse
        // row (23 cols) does fit and must appear intact.
        let mut app = App::demo();
        app.medium.as_mut().unwrap().temp_c = Some(1000.4);
        let color = TH.temp_color(1000.4);
        let buf = draw_buffer(30, 12, &app);
        // Sample the same 4 hero rows as bg-colour bitmaps rather than
        // grepping rendered text: filled hero pixels carry `color` as their
        // background now, and every cell's symbol is a plain space.
        let rows: Vec<String> = (0..4).map(|y| bg_bitmap_row(&buf, y, 30, color)).collect();
        let precise_row0 = super::font::big_text("1000.4°C").remove(0);
        let coarse_row0 = super::font::big_text("1000°C").remove(0);
        assert!(
            !rows.iter().any(|r| r.contains(precise_row0.as_str())),
            "full-precision hero row rendered intact — should have overflowed the 28-wide card"
        );
        assert!(
            rows.iter().any(|r| r.contains(coarse_row0.as_str())),
            "coarse fallback row missing — hero should fall back to \"1000°C\" when precise overflows"
        );
    }

    #[test]
    fn history_renders_block_sparkline() {
        // Drive the chart helper directly with an area exactly as wide as
        // the pushed history, so the whole buffer IS the chart: every column
        // is pinned to a known sample, not just "a bar appears somewhere".
        let mut t = Terminal::new(TestBackend::new(5, 1)).unwrap();
        let mut app = App::new(false);
        for v in [35.0_f32, 50.0, 70.0, 85.0, 100.0] {
            app.temp_hist.push(v);
        }
        t.draw(|f| super::render_chart(f, f.area(), &app, TH.text)).unwrap();
        let s: String = t.backend().buffer().content().iter().map(|c| c.symbol()).collect();
        // baseline-shift (v-30).clamp(0,75), then level = shifted*8/75:
        // 35→5→0(' '), 50→20→2(▂), 70→40→4(▄), 85→55→5(▅), 100→70(clamped 75 cap)→7(▇)
        assert_eq!(s, " ▂▄▅▇", "chart mis-scaled or mis-positioned");
        assert_eq!(
            s.chars().last().unwrap(),
            '▇',
            "newest sample (100°C, the tallest) must land at the right edge"
        );
    }
}
