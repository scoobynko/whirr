use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::Line;

/// 4-row quadrant glyphs, transcribed from the FIGlet `smblock` font. Strokes
/// are drawn with quadrant half-blocks (`▞▘▖▌▛`), which reads finer than a
/// full-block face and leaves more room for the unit beside the number —
/// digits are 3 cells wide here rather than 4. Every row of a glyph has equal
/// width.
///
/// `°` is hand-drawn: FIGlet fonts are ASCII-only and whirr needs it for
/// `88.0°C`. It is styled to match the transcribed glyphs rather than the
/// previous full-block face.
fn glyph(c: char) -> [&'static str; 4] {
    match c {
        '0' => ["▞▀▖", "▌▞▌", "▛ ▌", "▝▀ "],
        '1' => ["▗▌ ", " ▌ ", " ▌ ", "▝▀ "],
        '2' => ["▞▀▖", " ▗▘", "▗▘ ", "▀▀▘"],
        '3' => ["▞▀▖", " ▄▘", "▖ ▌", "▝▀ "],
        '4' => ["▌ ▌", "▚▄▌", "  ▌", "  ▘"],
        '5' => ["▛▀▘", "▙▄ ", "▖ ▌", "▝▀ "],
        '6' => ["▞▀▖", "▙▄ ", "▌ ▌", "▝▀ "],
        '7' => ["▛▀▌", " ▐ ", " ▌ ", " ▘ "],
        '8' => ["▞▀▖", "▚▄▘", "▌ ▌", "▝▀ "],
        '9' => ["▞▀▖", "▚▄▌", "▖ ▌", "▝▀ "],
        // Only the bottom two rows have ink (a low dot) — the top rows are
        // intentionally blank, not a missing glyph row.
        '.' => ["  ", "  ", "▗▖", "▝▘"],
        '°' => ["▞▖", "▝▘", "  ", "  "],
        'C' => ["▞▀▖", "▌  ", "▌ ▖", "▝▀ "],
        'W' => ["▌ ▌", "▌▖▌", "▙▚▌", "▘ ▘"],
        '%' => ["█ ▌", " ▞ ", "▞▗▖", "▘▝▘"],
        'G' => ["▞▀▖", "▌▄▖", "▌ ▌", "▝▀ "],
        // Centred on the second row, matching where the digits' waist sits.
        '-' => ["   ", "▄▄▖", "   ", "   "],
        ' ' => [" ", " ", " ", " "],
        _ => ["?", "?", "?", "?"],
    }
}

pub fn big_text(s: &str) -> Vec<String> {
    let mut rows = vec![String::new(); 4];
    for c in s.chars() {
        let g = glyph(c);
        for (i, row) in rows.iter_mut().enumerate() {
            row.push_str(g[i]);
            row.push(' ');
        }
    }
    rows
}

/// Render `precise` in the big font, falling back to `coarse` when the
/// rendered rows would overflow `width` — hero values must never truncate
/// mid-glyph. The quadrant face is narrow enough that no realistic reading
/// trips this any more (`100.0°C` is 26 cols against a 28-wide card); it now
/// guards only against implausible sensor output, e.g. `1000.0°C` at 30 cols.
pub fn big_text_fit(precise: &str, coarse: &str, width: u16) -> Vec<String> {
    let rows = big_text(precise);
    if rows[0].chars().count() <= width as usize {
        rows
    } else {
        big_text(coarse)
    }
}

/// The hero-number rendering shared by all four gauge cards: fit `precise`
/// (falling back to `coarse`) within `width`, then style each row as a
/// `Line` in `color`. Replaces the hand-rolled
/// `big_text(...).into_iter().map(|r| Line::styled(r, ...)).collect()` that
/// used to be duplicated across `cpu.rs`, `memory.rs`, `power.rs` and
/// `temp.rs`.
pub fn hero_lines(precise: &str, coarse: &str, width: u16, color: Color) -> Vec<Line<'static>> {
    big_text_fit(precise, coarse, width)
        .into_iter()
        .map(|r| Line::styled(r, Style::default().fg(color)))
        .collect()
}

/// Whether a card's inner area has room for a 4-row hero layout: 4 hero
/// rows plus a strip/chart below (height 9), and width for the hero string.
///
/// The width floor of 28 predates the quadrant font, which renders the widest
/// realistic hero (`88.0°C`) in 22 cols rather than ~27. The threshold is kept
/// as-is deliberately: lowering it would promote narrower cards into hero mode
/// and change which terminal sizes get the full-tier look, which is a design
/// decision rather than a consequence of the font swap.
pub fn hero_fits(inner: Rect) -> bool {
    inner.height >= 9 && inner.width >= 28
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;

    #[test]
    fn four_uniform_rows() {
        let rows = super::big_text("42.0 W");
        assert_eq!(rows.len(), 4);
        let w = rows[0].chars().count();
        assert!(rows.iter().all(|r| r.chars().count() == w));
    }

    #[test]
    fn all_required_glyphs_exist() {
        for c in "0123456789.°CW%G- ".chars() {
            let g = super::glyph(c);
            assert_eq!(g.len(), 4, "glyph {c:?} must have 4 rows");
            let w = g[0].chars().count();
            assert!(g.iter().all(|r| r.chars().count() == w), "glyph {c:?} rows uneven");
            assert_ne!(g, super::glyph('\u{1}'), "glyph {c:?} falls back to '?'");
        }
    }

    #[test]
    fn hero_fits_thresholds() {
        let r = |w, h| Rect::new(0, 0, w, h);
        assert!(super::hero_fits(r(28, 9)));
        assert!(!super::hero_fits(r(28, 8)));
        assert!(!super::hero_fits(r(27, 9)));
    }

    #[test]
    fn hero_lines_wraps_big_text_fit_with_styling() {
        use ratatui::style::Color;

        let lines = super::hero_lines("41%", "41%", 28, Color::Rgb(1, 2, 3));
        assert_eq!(lines.len(), 4);
        for (line, row) in lines.iter().zip(super::big_text("41%")) {
            assert_eq!(line.spans.len(), 1);
            assert_eq!(line.spans[0].content, row);
            assert_eq!(line.style.fg, Some(Color::Rgb(1, 2, 3)));
        }

        // Falls back to coarse exactly like `big_text_fit` when precise overflows.
        // With the narrower quadrant font, no realistic 3-integer-digit value
        // (e.g. "100.0°C") overflows a 28-wide card any more, so bump to a
        // 4-integer-digit value to keep exercising the real fallback path.
        let fallback = super::hero_lines("1000.0°C", "1000°C", 28, Color::Rgb(1, 2, 3));
        let coarse_rows = super::big_text("1000°C");
        for (line, row) in fallback.iter().zip(coarse_rows) {
            assert_eq!(line.spans[0].content, row);
        }
    }

    #[test]
    fn big_text_fit_uses_precise_when_it_fits() {
        let rows = super::big_text_fit("41%", "41%", 28);
        assert_eq!(rows, super::big_text("41%"));
    }

    #[test]
    fn big_text_fit_falls_back_to_coarse_when_precise_overflows() {
        // The quadrant font is narrower than the old full-block face: digits
        // are 3 cols instead of 4, so a realistic 3-integer-digit precise
        // value ("100.0°C", 26 cols) now fits a 28-wide card and no longer
        // exercises the fallback. Bump to a 4-integer-digit value to keep
        // proving the guard: "1000.0°C" renders 30 cols wide, still wider
        // than 28, so it must fall back to "1000°C" (23 cols).
        let rows = super::big_text_fit("1000.0°C", "1000°C", 28);
        assert!(rows[0].chars().count() <= 28);
        assert_eq!(rows, super::big_text("1000°C"));
        assert_ne!(rows, super::big_text("1000.0°C"));
    }
}
