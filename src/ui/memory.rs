use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::sampler::PressureLevel;
use crate::units::fmt_bytes;
use super::{font, theme};

pub fn segment_widths(parts: &[u64], width: u16) -> Vec<u16> {
    let total: u64 = parts.iter().sum();
    if total == 0 {
        return vec![0; parts.len()];
    }
    let mut widths: Vec<u16> = parts
        .iter()
        .map(|&p| {
            if p == 0 { 0 } else { ((p as f64 / total as f64) * width as f64).round().max(1.0) as u16 }
        })
        .collect();
    // reconcile rounding drift against the largest segment
    let mut diff = widths.iter().sum::<u16>() as i32 - width as i32;
    while diff != 0 {
        let i = widths
            .iter()
            .enumerate()
            .max_by_key(|(_, &w)| w)
            .map(|(i, _)| i)
            .unwrap();
        if diff > 0 { widths[i] -= 1; diff -= 1; } else { widths[i] += 1; diff += 1; }
    }
    widths
}

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let block = theme::panel_block("Memory", false);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let Some(mem) = app.medium.as_ref().and_then(|m| m.memory.as_ref()) else {
        f.render_widget(Paragraph::new("n/a").style(Style::default().fg(theme::DIM)), inner);
        return;
    };

    let state = match mem.pressure {
        PressureLevel::Normal => "NORMAL",
        PressureLevel::Warn => "WARN",
        PressureLevel::Critical => "CRITICAL",
    };
    let pcolor = theme::pressure_color(mem.pressure);

    let parts = [mem.app, mem.wired, mem.compressed, mem.free];
    let colors = [theme::ACCENT, theme::gradient(0.6), theme::AMBER, theme::BG_CELL];
    let labels = ["app", "wired", "compressed", "free"];
    let widths = segment_widths(&parts, inner.width.saturating_sub(3));

    let mut bar = Vec::new();
    for (i, (&w, &color)) in widths.iter().zip(colors.iter()).enumerate() {
        if w > 0 {
            bar.push(Span::styled("█".repeat(w as usize), Style::default().fg(color)));
            if i < widths.len() - 1 {
                bar.push(Span::raw(" "));
            }
        }
    }

    let swap = format!("swap {} / {}", fmt_bytes(mem.swap_used), fmt_bytes(mem.swap_total));

    if font::hero_fits(inner) {
        // The legend needs ~62 columns for all four entries on one line, but
        // the narrowest full-tier card is 28 wide (120-col terminal) — pack
        // it across as many rows as it needs instead of clipping mid-value.
        let used = mem.app + mem.wired + mem.compressed;
        let used_gib = used as f64 / 1_073_741_824.0;
        let precise = format!("{used_gib:.1}G");
        let coarse = format!("{used_gib:.0}G");
        let hero = font::hero_lines(&precise, &coarse, inner.width, pcolor);

        let mut body = vec![Line::from(bar)];
        body.extend(legend_lines(&labels, &colors, &parts, inner.width));
        body.push(Line::from(vec![
            Span::styled("pressure ", Style::default().fg(theme::DIM)),
            Span::styled(state, Style::default().fg(pcolor).bold()),
        ]));
        body.push(Line::styled(swap, Style::default().fg(theme::DIM)));

        let mut lines = hero;
        // Spend a spare row as a spacer between the hero number and the
        // detail rows when there's room, so wider cards (where the legend
        // fits in fewer rows) don't just sit half-empty.
        if lines.len() + body.len() < inner.height as usize {
            lines.push(Line::from(""));
        }
        lines.extend(body);
        f.render_widget(Paragraph::new(lines), inner);
    } else {
        let legend = Line::from(
            labels
                .iter()
                .zip(colors.iter())
                .zip(parts.iter())
                .flat_map(|((l, &c), &p)| {
                    vec![
                        Span::styled("■", Style::default().fg(c)),
                        Span::styled(format!(" {l} {} ", fmt_bytes(p)), Style::default().fg(theme::DIM)),
                    ]
                })
                .collect::<Vec<_>>(),
        );
        let lines = vec![
            Line::from(vec![
                Span::styled("pressure ", Style::default().fg(theme::DIM)),
                Span::styled(state, Style::default().fg(pcolor).bold()),
            ]),
            Line::from(bar),
            legend,
            Line::styled(swap, Style::default().fg(theme::DIM)),
        ];
        f.render_widget(Paragraph::new(lines), inner);
    }
}

/// Pack the legend entries greedily into as few lines as fit `width`, so a
/// narrow card wraps across several rows instead of clipping one long line.
/// At the narrowest full-tier card (28 cols) this takes 3 rows; from ~64
/// cols wide, all four entries share one line.
fn legend_lines(labels: &[&str], colors: &[Color], parts: &[u64], width: u16) -> Vec<Line<'static>> {
    let width = width as usize;
    let mut lines = Vec::new();
    let mut current: Vec<Span> = Vec::new();
    let mut current_w = 0usize;
    for ((&l, &c), &p) in labels.iter().zip(colors.iter()).zip(parts.iter()) {
        let text = format!(" {l} {} ", fmt_bytes(p));
        let item_w = 1 + text.chars().count(); // "■" marker + text
        if current_w > 0 && current_w + item_w > width {
            lines.push(Line::from(std::mem::take(&mut current)));
            current_w = 0;
        }
        current.push(Span::styled("■", Style::default().fg(c)));
        current.push(Span::styled(text, Style::default().fg(theme::DIM)));
        current_w += item_w;
    }
    if !current.is_empty() {
        lines.push(Line::from(current));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::segment_widths;
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
    fn hero_shows_used_gib_when_room() {
        // demo used = 4G + 2G + 1G = 7_000_000_000 B = 6.5 GiB → "6.5G", in
        // GREEN (demo pressure is Normal). The hero digits are
        // background-filled cells now (see `ui/font.rs`'s doc comment), so
        // verify the "6.5G" bitmap landed by counting pressure-colour-bg
        // cells rather than grepping rendered text for glyph characters.
        let full_buf = draw_buffer(40, 12);
        let filled =
            full_buf.content().iter().filter(|c| c.style().bg == Some(super::theme::GREEN)).count();
        let expected: usize =
            crate::ui::font::big_text("6.5G").iter().flat_map(|r| r.chars()).filter(|&c| c == '#').count();
        assert_eq!(filled, expected, "hero bitmap pixel count mismatch for \"6.5G\"");

        let full = draw(40, 12);
        assert!(full.contains("pressure "), "pressure label missing");
        assert!(full.contains("NORMAL"), "pressure state missing");
        assert!(full.contains("swap 0 B / 953.7 MB"), "swap line missing or clipped");

        let compact_buf = draw_buffer(40, 10);
        let compact_filled =
            compact_buf.content().iter().any(|c| c.style().bg == Some(super::theme::GREEN));
        assert!(!compact_filled, "compact tier must not paint any hero bitmap pixels");
        let compact = draw(40, 10);
        assert!(compact.contains("pressure "));
    }

    /// Regression test for the 120x30 full-tier clip: a gauge card there is
    /// `width/4 - 2` wide, so at 120 cols the Memory card's inner width is
    /// exactly 28 — the narrowest the hero layout ever gets. The legend
    /// needs ~62 columns for all four entries on one line; it must wrap
    /// across the free rows below the hero number instead of clipping.
    #[test]
    fn nothing_clips_at_the_narrowest_full_tier_card_width() {
        // 120 / 4 gauges = 30 wide, inner 28; gauge-row height 12, inner 10
        // — the real dimensions of a full-tier Memory card at 120x30.
        let card = draw(30, 12);
        for needle in [
            "app 3.7 GB",
            "wired 1.9 GB",
            "compressed 953.7 MB",
            "free 8.4 GB",
            "pressure ",
            "NORMAL",
            "swap 0 B / 953.7 MB",
        ] {
            assert!(card.contains(needle), "clipped or missing at 28-wide card: {needle:?}");
        }
    }

    /// Same check at the card widths a 160x45 and 200x50 terminal give the
    /// Memory card (40 and 50 cols, inner 38 and 48) — the fix for the
    /// 120-wide case must not have broken wider cards.
    #[test]
    fn nothing_clips_at_wider_full_tier_card_widths() {
        for w in [40u16, 50] {
            let card = draw(w, 12);
            for needle in [
                "app 3.7 GB",
                "wired 1.9 GB",
                "compressed 953.7 MB",
                "free 8.4 GB",
                "pressure ",
                "NORMAL",
                "swap 0 B / 953.7 MB",
            ] {
                assert!(card.contains(needle), "{w} wide: clipped or missing: {needle:?}");
            }
        }
    }

    #[test]
    fn widths_sum_and_respect_minimums() {
        let w = segment_widths(&[50, 30, 15, 5], 40);
        assert_eq!(w.iter().sum::<u16>(), 40);
        assert!(w.iter().all(|&x| x >= 1));
        assert!(w[0] > w[3]);
    }

    #[test]
    fn zero_parts_get_zero() {
        let w = segment_widths(&[100, 0, 100], 20);
        assert_eq!(w[1], 0);
        assert_eq!(w.iter().sum::<u16>(), 20);
    }
}
