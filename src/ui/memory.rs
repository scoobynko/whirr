use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::sampler::PressureLevel;
use crate::units::{fmt_bytes, fmt_gib};
use super::{font, gauge, theme};

/// Split `width` cells across `parts` in proportion to their sizes, summing to
/// exactly `width`.
///
/// Every non-zero part gets at least one cell, so a small segment still shows
/// its colour instead of disappearing. The previous version promised that too,
/// then broke it while reconciling rounding drift: it decremented whichever
/// segment was currently widest, with no floor, so `[1,1,1,1]` across 2 cells
/// came back as `[1,1,0,0]` — two segments silently gone.
///
/// When there are more non-zero parts than cells, that promise is
/// unkeepable — so the largest parts take the cells and the smallest go
/// unrendered, which is the least misleading way to run out of room.
pub fn segment_widths(parts: &[u64], width: u16) -> Vec<u16> {
    let total: u64 = parts.iter().sum();
    let mut widths = vec![0u16; parts.len()];
    if total == 0 || width == 0 {
        return widths;
    }

    // Largest first, so both the shortfall case and the leftover-cell
    // tie-break below resolve toward the segments that matter most.
    let mut order: Vec<usize> = (0..parts.len()).filter(|&i| parts[i] > 0).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(parts[i]));

    if width as usize <= order.len() {
        for &i in order.iter().take(width as usize) {
            widths[i] = 1;
        }
        return widths;
    }

    // One cell each, then share out what's left by largest remainder — the
    // standard apportionment, which sums to `width` exactly by construction
    // rather than by a fixup loop afterwards.
    let extra = width as usize - order.len();
    let mut remainders: Vec<(usize, f64)> = Vec::with_capacity(order.len());
    let mut handed_out = 0usize;
    for &i in &order {
        let exact = parts[i] as f64 / total as f64 * extra as f64;
        let floor = exact.floor();
        widths[i] = 1 + floor as u16;
        handed_out += floor as usize;
        remainders.push((i, exact - floor));
    }
    // Ties keep `order`'s largest-part-first sequence: sort_by is stable.
    remainders.sort_by(|a, b| b.1.total_cmp(&a.1));
    for &(i, _) in remainders.iter().take(extra - handed_out) {
        widths[i] += 1;
    }
    widths
}

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    let inner = gauge::frame(f, area, &app.theme, "Memory");
    let Some(mem) = app.medium.as_ref().and_then(|m| m.memory.as_ref()) else {
        return gauge::unavailable(f, inner, &app.theme);
    };

    let state = match mem.pressure {
        PressureLevel::Normal => "NORMAL",
        PressureLevel::Warn => "WARN",
        PressureLevel::Critical => "CRITICAL",
    };
    let pcolor = app.theme.pressure_color(mem.pressure);

    let parts = [mem.app, mem.wired, mem.compressed, mem.free];
    let colors = [app.theme.accent, app.theme.gradient(0.6), app.theme.amber, app.theme.bg_cell];
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
        let precise = fmt_gib(used);
        let coarse = format!("{:.0}G", used as f64 / 1024.0_f64.powi(3));
        let hero = font::hero_lines(&precise, &coarse, inner.width, pcolor);

        let mut body = legend_lines(&app.theme, &labels, &colors, &parts, inner.width);
        let avail = inner.height as usize;

        let pressure_span = Span::styled(state, Style::default().fg(pcolor).bold());
        let separate = [
            Line::from(vec![Span::styled("pressure ", Style::default().fg(app.theme.dim)), pressure_span.clone()]),
            Line::styled(swap, Style::default().fg(app.theme.dim)),
        ];

        // At the narrowest full-tier card (28 cols) the legend alone needs 3
        // rows, and hero(5) + pressure(1) + swap(1) already exactly fill the
        // 10-row inner area, leaving no room for the bar — the card's
        // signature element. Recover a row by folding pressure and swap onto
        // one line, in GiB (`64.0G` rather than `64.0 GB`). Even with the
        // longest pressure word (CRITICAL) and a triple-digit-GiB swap total
        // that's 28 chars — exactly the narrowest width — but drop the
        // "swap" label as a fallback so it still can't clip mid-value if
        // swap ever grows past that.
        let (used_g, total_g) = (fmt_gib(mem.swap_used), fmt_gib(mem.swap_total));
        let full_tail = format!(" · swap {used_g}/{total_g}");
        let short_tail = format!(" · {used_g}/{total_g}");
        let tail = if state.len() + full_tail.chars().count() <= inner.width as usize {
            full_tail
        } else {
            short_tail
        };
        let merged = [Line::from(vec![pressure_span, Span::styled(tail, Style::default().fg(app.theme.dim))])];

        // Prefer the full two-line pressure/swap detail — it's more
        // informative and is what wider cards (where the legend packs into
        // fewer rows) already have room for. Only fold onto one line when
        // that's what it takes to keep the bar on screen.
        if hero.len() + 1 + body.len() + separate.len() <= avail {
            body.extend(separate);
        } else {
            body.extend(merged);
        }

        let mut lines = hero;
        // The segmented usage bar is the card's signature element and must
        // always render at every full-tier width; the merge above exists
        // precisely so this fits at the narrowest one (120-col terminals).
        if lines.len() + 1 + body.len() <= avail {
            lines.push(Line::from(bar));
        }
        // Spend a spare row as a spacer between the hero number and the
        // detail rows when there's room, so wider cards (where the legend
        // fits in fewer rows) don't just sit half-empty.
        if lines.len() + body.len() < avail {
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
                        Span::styled(format!(" {l} {} ", fmt_bytes(p)), Style::default().fg(app.theme.dim)),
                    ]
                })
                .collect::<Vec<_>>(),
        );
        let lines = vec![
            Line::from(vec![
                Span::styled("pressure ", Style::default().fg(app.theme.dim)),
                Span::styled(state, Style::default().fg(pcolor).bold()),
            ]),
            Line::from(bar),
            legend,
            Line::styled(swap, Style::default().fg(app.theme.dim)),
        ];
        f.render_widget(Paragraph::new(lines), inner);
    }
}

/// Pack the legend entries greedily into as few lines as fit `width`, so a
/// narrow card wraps across several rows instead of clipping one long line.
/// At the narrowest full-tier card (28 cols) this takes 3 rows; from ~64
/// cols wide, all four entries share one line.
fn legend_lines(
    theme: &theme::Theme,
    labels: &[&str],
    colors: &[Color],
    parts: &[u64],
    width: u16,
) -> Vec<Line<'static>> {
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
        current.push(Span::styled(text, Style::default().fg(theme.dim)));
        current_w += item_w;
    }
    if !current.is_empty() {
        lines.push(Line::from(current));
    }
    lines
}

#[cfg(test)]
mod tests {
    /// The palette the app starts with; tests assert against it by name
    /// rather than against raw RGB triples.
    const TH: crate::ui::theme::Theme = crate::ui::theme::Theme::dark();

    use super::segment_widths;
    use ratatui::backend::TestBackend;
    use ratatui::buffer::Buffer;
    use ratatui::Terminal;

    use crate::app::App;
    use crate::sampler::{MediumSnap, MemDetail, PressureLevel, Snapshot};

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

    /// Build an `App` carrying only a custom `MemDetail`, so tests can drive
    /// the Memory card with values `App::demo()` doesn't cover (e.g. a large
    /// swap total) without dragging in the rest of the demo snapshot.
    fn app_with_mem(mem: MemDetail) -> App {
        let mut app = App::new(false);
        app.ingest(Snapshot::Medium(MediumSnap {
            temp_c: None,
            power: None,
            battery: None,
            memory: Some(mem),
            uptime_secs: 0,
        }));
        app
    }

    fn draw_mem(w: u16, h: u16, mem: MemDetail) -> String {
        let app = app_with_mem(mem);
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        t.backend().buffer().content().iter().map(|c| c.symbol()).collect()
    }

    /// Count of "█" cells in a rendered card — the segmented usage bar's
    /// signature glyph. Zero means the bar didn't render at all.
    fn bar_cell_count(rendered: &str) -> usize {
        rendered.matches('█').count()
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
            full_buf.content().iter().filter(|c| c.style().bg == Some(TH.green)).count();
        let expected: usize =
            crate::ui::font::big_text("6.5G").iter().flat_map(|r| r.chars()).filter(|&c| c == '#').count();
        assert_eq!(filled, expected, "hero bitmap pixel count mismatch for \"6.5G\"");

        let full = draw(40, 12);
        assert!(full.contains("pressure "), "pressure label missing");
        assert!(full.contains("NORMAL"), "pressure state missing");
        assert!(full.contains("swap 0 B / 953.7 MB"), "swap line missing or clipped");

        let compact_buf = draw_buffer(40, 10);
        let compact_filled =
            compact_buf.content().iter().any(|c| c.style().bg == Some(TH.green));
        assert!(!compact_filled, "compact tier must not paint any hero bitmap pixels");
        let compact = draw(40, 10);
        assert!(compact.contains("pressure "));
    }

    /// Regression test for the 120x30 full-tier clip: a gauge card there is
    /// `width/4 - 2` wide, so at 120 cols the Memory card's inner width is
    /// exactly 28 — the narrowest the hero layout ever gets. The legend
    /// needs ~62 columns for all four entries on one line; it must wrap
    /// across the free rows below the hero number instead of clipping. At
    /// this width, pressure and swap are also folded onto one merged line
    /// (see `render`) to free the row the segmented usage bar needs.
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
            "NORMAL",
            "swap 0.0G/0.9G",
        ] {
            assert!(card.contains(needle), "clipped or missing at 28-wide card: {needle:?}");
        }
        assert!(bar_cell_count(&card) > 0, "usage bar must render at the 120-col card width");
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
            assert!(bar_cell_count(&card) > 0, "{w} wide: usage bar must render");
        }
    }

    /// Pin the usage bar's presence explicitly at both full-tier reference
    /// widths named in the fix: the 120-col terminal (card inner 28, merged
    /// pressure/swap) and the 160-col terminal (card inner 38, unmerged).
    /// Losing the bar at 120 cols — the most common full-tier size — was the
    /// regression; it must never silently disappear again.
    #[test]
    fn usage_bar_present_at_120_and_160_col_reference_widths() {
        let card_120 = draw(30, 12); // 120 / 4 gauges = 30 wide, inner 28
        let card_160 = draw(40, 12); // 160 / 4 gauges = 40 wide, inner 38
        assert!(bar_cell_count(&card_120) > 0, "bar missing at 120-col card width");
        assert!(bar_cell_count(&card_160) > 0, "bar missing at 160-col card width");
    }

    /// Swap can run to tens of GB; verify the merged narrow-width line built
    /// from `fmt_bytes`-scale figures still fits the 28-col inner width
    /// without clipping mid-value, and that the bar still has room next to
    /// it. Also exercises the longest pressure word (CRITICAL).
    #[test]
    fn merged_pressure_swap_line_fits_with_a_large_swap_value() {
        // Same app/wired/compressed/free magnitudes as `App::demo()` (so the
        // legend still packs into its usual 3 rows at this width) — only
        // swap and pressure are pushed to their stress values, to isolate
        // what the merged line itself does under a large swap total.
        let mem = MemDetail {
            app: 4_000_000_000,
            wired: 2_000_000_000,
            compressed: 1_000_000_000,
            free: 9_000_000_000,
            swap_used: 64 * 1_073_741_824,
            swap_total: 128 * 1_073_741_824,
            pressure: PressureLevel::Critical,
        };
        let card = draw_mem(30, 12, mem);
        assert!(card.contains("CRITICAL"), "pressure state missing");
        // The merged tail keeps its "swap" label here (28 chars exactly at
        // this width) — asserting the full, unbroken figure pair proves
        // neither value was cut off mid-number.
        assert!(card.contains("swap 64.0G/128.0G"), "swap figures missing or clipped: {card:?}");
        assert!(bar_cell_count(&card) > 0, "usage bar must still render alongside a large swap value");
    }

    /// If a merged line ever can't fit even with the "swap" label dropped
    /// (implausibly large swap totals), the fallback must still print both
    /// figures whole rather than truncate one mid-digit.
    #[test]
    fn merged_pressure_swap_line_drops_label_rather_than_truncate() {
        let mem = MemDetail {
            app: 4_000_000_000,
            wired: 2_000_000_000,
            compressed: 1_000_000_000,
            free: 9_000_000_000,
            swap_used: 64 * 1_073_741_824,
            swap_total: 1_200 * 1_073_741_824,
            pressure: PressureLevel::Critical,
        };
        let card = draw_mem(30, 12, mem);
        assert!(card.contains("64.0G/1200.0G"), "swap figures missing or clipped: {card:?}");
        assert!(bar_cell_count(&card) > 0, "usage bar must still render");
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

    /// The reconcile loop this replaced would decrement whichever segment was
    /// widest, with no floor, so a non-zero part could be reconciled down to
    /// zero cells and vanish from the bar. Swept rather than spot-checked,
    /// since the failure only showed up at particular width/part combinations.
    #[test]
    fn a_nonzero_part_keeps_a_cell_whenever_the_width_allows_one() {
        let cases: [&[u64]; 5] = [
            &[1, 1, 1, 1],
            &[10, 1, 1, 1],
            &[100, 1, 1, 1],
            &[4_000_000_000, 2_000_000_000, 1_000_000_000, 9_000_000_000],
            &[1, 0, 1, 0],
        ];
        for parts in cases {
            let nonzero = parts.iter().filter(|&&p| p > 0).count();
            for width in 0u16..=64 {
                let got = segment_widths(parts, width);
                assert_eq!(
                    got.iter().sum::<u16>(),
                    width,
                    "{parts:?} at {width}: widths must sum to the full width"
                );
                for (&p, &w) in parts.iter().zip(&got) {
                    assert!(p > 0 || w == 0, "{parts:?} at {width}: a zero part took cells");
                }
                if width as usize >= nonzero {
                    assert!(
                        parts.iter().zip(&got).all(|(&p, &w)| p == 0 || w >= 1),
                        "{parts:?} at {width}: a non-zero segment was squeezed to 0 cells: {got:?}"
                    );
                }
            }
        }
    }

    /// When there are fewer cells than non-zero parts the minimum cannot be
    /// kept at all. The cells must go to the largest parts rather than to
    /// whichever segment rounding happened to favour.
    #[test]
    fn too_few_cells_keeps_the_largest_parts() {
        assert_eq!(segment_widths(&[1, 9, 2, 8], 2), vec![0, 1, 0, 1]);
        assert_eq!(segment_widths(&[5, 1, 1, 1], 1), vec![1, 0, 0, 0]);
        assert_eq!(segment_widths(&[5, 1, 1, 1], 0), vec![0, 0, 0, 0]);
    }
}
