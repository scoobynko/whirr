use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

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

    let hero = font::hero_fits(inner);
    let rows = Layout::vertical([
        Constraint::Length(if hero { 5 } else { 1 }), // hero
        Constraint::Length(1),                         // cpu/gpu/ane legend
        Constraint::Min(2),                            // total sparkline
        Constraint::Length(1),                         // battery footer
    ])
    .split(inner);

    match &m.power {
        Some(p) => {
            let total = p.cpu_w + p.gpu_w + p.ane_w;
            let text = format!("{total:.1} W");
            if hero {
                let coarse = format!("{total:.0} W");
                let lines = font::hero_lines(&text, &coarse, inner.width, theme::ACCENT);
                f.render_widget(Paragraph::new(lines), rows[0]);
            } else {
                f.render_widget(
                    Paragraph::new(Span::styled(text, Style::default().fg(theme::ACCENT).bold())),
                    rows[0],
                );
            }
            let legend = format!("cpu {:.1} · gpu {:.1} · ane {:.1}", p.cpu_w, p.gpu_w, p.ane_w);
            f.render_widget(
                Paragraph::new(legend).style(Style::default().fg(theme::DIM)),
                rows[1],
            );
            render_spark(f, rows[2], app);
        }
        None => {
            f.render_widget(Paragraph::new("n/a").style(Style::default().fg(theme::DIM)), rows[0]);
        }
    }

    let battery_line = match &m.battery {
        Some(b) => {
            // Kept terse ("cyc"/"h" rather than "cycles"/"health") so the
            // full line — percent, cycles and health — still fits the
            // narrowest full-tier card (inner width 28 at 120 cols); the
            // old wording clipped at widths at or below ~124.
            // Geometric arrows, not emoji: colour emoji clash with the
            // monochrome palette and render double-width, which shifted every
            // following field by a cell. ↑/↓ are single-width and match the
            // network card's directional vocabulary.
            let state = if b.charging { "↑" } else { "↓" };
            let health = b.health_pct.map_or(String::new(), |h| format!(" · h{h}%"));
            format!("{state} {}% · {}cyc{health}", b.percent, b.cycles)
        }
        None => String::new(), // desktop Mac: hide line
    };
    f.render_widget(
        Paragraph::new(battery_line).style(Style::default().fg(theme::DIM)),
        rows[3],
    );
}

/// Filled block sparkline of total watts (cpu + gpu + ane).
fn render_spark(f: &mut Frame, area: Rect, app: &App) {
    let data: Vec<u64> = app
        .power_hist
        .iter()
        .map(|(c, g, a)| ((c + g + a) * 10.0).round() as u64)
        .collect();
    if data.is_empty() {
        return;
    }
    let peak = app.power_hist.iter().map(|(c, g, a)| c + g + a).fold(1.0, f64::max) * 1.2;
    let max = (peak * 10.0).round() as u64;
    super::spark::render(f, area, &data, max, Style::default().fg(theme::ACCENT));
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::Terminal;

    use crate::app::App;

    fn draw(w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
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
    fn hero_when_room_compact_when_small() {
        // demo power total = 6.4 + 1.2 + 0.3 = 7.9 → "7.9 W". The hero
        // digits are background-filled cells now (see `ui/font.rs`'s doc
        // comment for why), so verify the "7.9 W" bitmap landed by counting
        // accent-bg cells, not by grepping rendered text for glyph chars.
        let full = draw(40, 12); // inner 38x10 → hero
        let full_buf = draw_buffer(40, 12);
        let filled =
            full_buf.content().iter().filter(|c| c.style().bg == Some(super::theme::ACCENT)).count();
        let expected: usize =
            crate::ui::font::big_text("7.9 W").iter().flat_map(|r| r.chars()).filter(|&c| c == '#').count();
        assert_eq!(filled, expected, "hero bitmap pixel count mismatch for \"7.9 W\"");
        assert!(full.contains("cpu 6.4"), "power legend (cpu/gpu/ane) missing");

        let compact = draw(40, 10); // inner 38x8 → compact
        assert!(compact.contains("7.9 W"));
        let compact_buf = draw_buffer(40, 10);
        let compact_filled =
            compact_buf.content().iter().any(|c| c.style().bg == Some(super::theme::ACCENT));
        assert!(!compact_filled, "compact tier must not paint any hero bitmap pixels");
    }

    #[test]
    fn battery_footer_fits_at_120_width_card() {
        // Real full-tier card size at 120 total cols with all 4 gauges shown
        // (120/4=30 wide, inner 28): the old "cycles"/"health" wording
        // clipped at widths at or below ~124.
        let full = draw(30, 12);
        // "↑" is single-width, unlike the emoji this replaced — one space.
        assert!(
            full.contains("↑ 76% · 120cyc · h97%"),
            "battery footer clipped or missing at inner width 28"
        );
    }

    #[test]
    fn history_renders_block_sparkline() {
        // Drive the chart helper directly with an area exactly as wide as
        // the pushed history, so the whole buffer IS the chart: every column
        // is pinned to a known sample, not just "a bar appears somewhere".
        let mut t = Terminal::new(TestBackend::new(4, 1)).unwrap();
        let mut app = App::new(false);
        // totals (cpu+gpu+ane): 1.0, 3.0, 5.0, 10.0 W; peak = 10.0*1.2 = 12.0 W
        for v in [(0.6_f64, 0.3, 0.1), (2.0, 0.8, 0.2), (3.5, 1.2, 0.3), (7.0, 2.5, 0.5)] {
            app.power_hist.push(v);
        }
        t.draw(|f| super::render_spark(f, f.area(), &app)).unwrap();
        let s: String = t.backend().buffer().content().iter().map(|c| c.symbol()).collect();
        // scaled data = round(total*10); max = round(peak*10) = 120.
        // level = scaled*8/120: 10→0(' '), 30→2(▂), 50→3(▃), 100→6(▆)
        assert_eq!(s, " ▂▃▆", "chart mis-scaled or mis-positioned");
        assert_eq!(
            s.chars().last().unwrap(),
            '▆',
            "newest sample (10 W, the tallest) must land at the right edge"
        );
    }
}

