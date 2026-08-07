use ratatui::prelude::*;



/// Block levels from empty to full, indexed by eighths (0..=8).
const LEVELS: [&str; 9] = [" ", "▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

/// Filled block sparkline (`▁▂▃▄▅▆▇█`) with each bar's colour ramped by its
/// own height — dim near the baseline, the full `style` colour at the peak —
/// instead of every bar being one flat shade. `data` is oldest→newest; only
/// the most recent `area.width` samples are drawn so the newest lands at the
/// right edge (the raw oldest-first history is tail-sliced here).
///
/// When there's less history than the area is wide, the short slice is drawn
/// right-anchored instead of left-aligned — the newest sample must stay at
/// the right edge, and the empty left space must stay genuinely blank rather
/// than fake zero-value bars.
pub fn render(
    f: &mut Frame,
    area: Rect,
    theme: &super::theme::Theme,
    data: &[u64],
    max: u64,
    style: Style,
) {
    if area.width == 0 || area.height == 0 || data.is_empty() {
        return;
    }
    let max = max.max(1);
    let w = area.width as usize;
    let tail = if data.len() > w { &data[data.len() - w..] } else { data };
    if tail.is_empty() {
        return;
    }
    // Clamp so a pathological value can't overflow the `value * height * 8 /
    // max` height computation below.
    let clamped: Vec<u64> = tail.iter().map(|&v| v.min(max)).collect();
    // Right-anchor short slices instead of ratatui's default left alignment.
    let x0 = area.x + (w - clamped.len()) as u16;

    let buf = f.buffer_mut();
    for (i, &value) in clamped.iter().enumerate() {
        let x = x0 + i as u16;
        // Same bottom-up fill ratatui's Sparkline used: each row consumes up
        // to 8 eighths of `height`, starting from the bottom row.
        let mut height = value * u64::from(area.height) * 8 / max;
        let bar_colour = style.fg.map(|fg| theme.ramp(fg, value as f32 / max as f32));
        let bar_style = match bar_colour {
            Some(c) => style.fg(c),
            None => style,
        };
        for j in (0..area.height).rev() {
            let level = height.min(8);
            height = height.saturating_sub(8);
            let cell = buf.cell_mut((x, area.y + j)).expect("cell within the sparkline's own area");
            match (level, bar_colour) {
                // A completely full cell is painted as a cell *background*
                // rather than a '█' foreground glyph. Terminal.app draws block
                // glyphs from the font's outlines instead of as native
                // rectangles, so a column of stacked '█' shows a hairline seam
                // between every row; a cell background is always painted as a
                // full rectangle — the terminal never consults the font for it
                // — so it cannot seam. Same fix and same reason as the hero
                // font; see `ui/font.rs`'s doc comment.
                (8, Some(colour)) => {
                    cell.set_symbol(" ");
                    cell.set_style(Style::default().bg(colour));
                }
                // Partial cells keep a foreground block glyph: it's the only
                // way to get sub-cell resolution, and a bar's topmost cell has
                // no filled cell above it to seam against. Empty cells (level
                // 0) render as a plain space, leaving the frame's BASE.
                //
                // A caller that supplies no foreground colour has nothing to
                // fill a background with, so it falls back to the glyph for
                // full cells too. Every real caller passes a colour.
                _ => {
                    cell.set_symbol(LEVELS[level as usize]);
                    cell.set_style(bar_style);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    /// The palette the app starts with; tests assert against it by name
    /// rather than against raw RGB triples.
    const TH: crate::ui::theme::Theme = crate::ui::theme::Theme::dark();

    use ratatui::backend::TestBackend;
    use ratatui::prelude::*;
    use ratatui::Terminal;


    use ratatui::style::Color;

    /// Render with a real foreground colour, as every production caller does.
    /// That matters now: a completely full cell is painted as a background
    /// fill rather than a '█' glyph, and that path only engages when there is
    /// a colour to fill with. Returns the symbol dump alongside a per-cell
    /// "is this a filled background" map, so assertions can talk about full
    /// bars without looking for a '█' the renderer deliberately no longer
    /// emits.
    fn draw_cells(data: &[u64], max: u64, w: u16, h: u16) -> (String, Vec<bool>) {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| super::render(f, f.area(), &TH, data, max, Style::default().fg(TH.accent)))
            .unwrap();
        let buf = t.backend().buffer().clone();
        let symbols = buf.content().iter().map(|c| c.symbol()).collect();
        let filled = buf
            .content()
            .iter()
            .map(|c| !matches!(c.style().bg, None | Some(Color::Reset)))
            .collect();
        (symbols, filled)
    }

    fn draw(data: &[u64], max: u64, w: u16, h: u16) -> String {
        draw_cells(data, max, w, h).0
    }

    #[test]
    fn renders_expected_block_levels() {
        // values 0..=8 against max 8 used to fill one row as " ▁▂▃▄▅▆▇█".
        // The last cell is completely full, so it is now a background fill and
        // its symbol is a space — the seven partial levels are unchanged.
        let (s, filled) = draw_cells(&[0, 1, 2, 3, 4, 5, 6, 7, 8], 8, 9, 1);
        assert_eq!(s, " ▁▂▃▄▅▆▇ ");
        assert_eq!(
            filled,
            vec![false, false, false, false, false, false, false, false, true],
            "only the completely full cell should be a background fill"
        );
    }

    /// The whole point of the background fill: a solid column of bar must not
    /// emit stacked '█' glyphs, because Terminal.app draws those from font
    /// outlines and seams between every row (see `render`'s comment and
    /// `ui/font.rs`). A full-height bar in a tall chart is the worst case.
    #[test]
    fn full_bars_emit_no_stacked_block_glyphs() {
        let (s, filled) = draw_cells(&[8], 8, 1, 6);
        assert!(!s.contains('█'), "solid bar still drawn with '█' glyphs: {s:?}");
        assert!(filled.iter().all(|&f| f), "every cell of a full-height bar should be filled");
    }

    #[test]
    fn shows_only_the_most_recent_width_samples() {
        // 100 ascending samples into a width-10 chart show the last 10
        // (samples 91..=100), newest at the right edge.
        let data: Vec<u64> = (1..=100).collect();
        let (s, filled) = draw_cells(&data, 100, 10, 1);
        assert!(*filled.last().unwrap(), "newest (100) should be a full, background-filled cell at right");
        // sample 91/100 → 7/8 height → '▇'; the first 10 samples (1..=10) would
        // be near-empty, so a filled left cell proves we tail-sliced.
        assert!("▅▆▇█".contains(s.chars().next().unwrap()), "left cell should be the recent tail, not the oldest samples");
    }

    #[test]
    fn short_data_right_anchors_instead_of_left_aligning() {
        // 3 samples into a 10-wide chart: bars must sit flush against the
        // RIGHT edge (newest at the far right), and the left 7 cells must
        // stay genuinely blank — not zero-value bars.
        let s = draw(&[2, 5, 8], 10, 10, 1);
        assert_eq!(s, "       ▁▄▆", "bars must hug the right edge with blank space on the left");
    }

    #[test]
    fn data_exactly_equal_to_width_fills_the_whole_area() {
        // Trailing cell is at max, so it is a background fill (space), not '█'.
        let (s, filled) = draw_cells(&[1, 2, 3, 4, 5], 5, 5, 1);
        assert_eq!(s, "▁▃▄▆ ", "no blank padding when data fills the area exactly");
        assert_eq!(filled, vec![false, false, false, false, true]);
    }

    #[test]
    fn values_at_or_above_max_are_clamped_not_overflowed() {
        // A pathological value far above max must not panic or wrap; it
        // should render as a full bar, same as a value exactly at max.
        let (_, filled) = draw_cells(&[u64::MAX, 5], 10, 2, 1);
        assert!(filled[0], "value above max should clamp to a full (background-filled) bar");
    }

    #[test]
    fn max_zero_does_not_panic_or_divide_by_zero() {
        let s = draw(&[1, 2, 3], 0, 3, 1);
        assert_eq!(s.chars().count(), 3);
    }

    #[test]
    fn empty_data_does_not_panic() {
        let s = draw(&[], 10, 5, 1);
        assert_eq!(s, "     ", "an empty dataset should render as entirely blank, not panic");
    }

    #[test]
    fn bar_colour_ramps_with_height_not_flat() {
        // Two bars in the same chart: a short one (1/8) and the tallest
        // possible one (8/8). If colour were still flat (the old Sparkline
        // behaviour) both cells would carry the exact same fg. A real ramp
        // must make the tall bar visibly brighter than the short one.
        let mut t = Terminal::new(TestBackend::new(2, 1)).unwrap();
        t.draw(|f| {
            super::render(f, f.area(), &TH, &[1, 8], 8, Style::default().fg(TH.accent))
        })
        .unwrap();
        let buf = t.backend().buffer().clone();
        // The short bar (1/8) is a partial cell, so its colour is a foreground;
        // the tall bar (8/8) is completely full, so its colour is a background.
        let short = buf[(0, 0)].fg;
        let tall = buf[(1, 0)].bg;
        assert_ne!(short, tall, "short and tall bars in the same chart must not share a flat colour");
        assert_eq!(tall, TH.accent, "the tallest bar should render in the full caller colour");
    }

    #[test]
    fn lowest_nonzero_bar_stays_distinct_from_base() {
        // A single-eighth bar against a large max (finest possible non-zero
        // fraction of the peak) must still render clearly apart from BASE,
        // not fade into the background it's blended toward.
        let mut t = Terminal::new(TestBackend::new(1, 8)).unwrap();
        t.draw(|f| super::render(f, f.area(), &TH, &[1], 64, Style::default().fg(TH.accent)))
            .unwrap();
        let buf = t.backend().buffer().clone();
        let fg = buf[(0, 7)].fg; // bottom row: the one filled eighth
        let ch = |c: Color| match c {
            Color::Rgb(r, g, b) => (i32::from(r), i32::from(g), i32::from(b)),
            _ => panic!(),
        };
        let (r0, g0, b0) = ch(TH.base);
        let (r1, g1, b1) = ch(fg);
        let dist = (r1 - r0).abs() + (g1 - g0).abs() + (b1 - b0).abs();
        assert!(dist > 20, "lowest non-zero bar colour {fg:?} too close to BASE {:?}", TH.base);
    }
}
