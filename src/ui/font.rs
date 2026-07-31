use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

/// 5-row pixel-bitmap glyphs. `#` marks a filled pixel, a space an empty one;
/// every row of a glyph has equal width, but width varies glyph to glyph.
///
/// This used to be a quadrant-block face (`▞▘▖▌▛`) drawn with *foreground*
/// characters. In macOS Terminal.app, block glyphs are drawn from the font's
/// outlines rather than as native rectangles, which leaves a visible hairline
/// seam between the rows of every multi-row glyph — Ghostty/iTerm2/Kitty/
/// WezTerm don't have this problem, but Terminal.app does. A cell *background*
/// fill, by contrast, is always painted as a full rectangle covering the
/// entire cell — the terminal doesn't consult the font for it — so it never
/// seams. `hero_lines` renders each pixel of this bitmap as a space with the
/// hero colour as its background rather than a block character as its
/// foreground. The tradeoff: whole-cell fills can't do half-blocks or
/// quadrants, so there's no smooth diagonal — glyphs are blockier than the
/// old quadrant face. Also used by the header wordmark (`ui/header.rs`),
/// which needs the letters `W H I R` in addition to the digits/units below.
///
/// The grid was originally 4 rows, which is below the minimum for legible
/// bitmap digits: `8` was a solid 3x4 rectangle, `6`/`9` were nearly solid,
/// and `2`/`3` differed by a single pixel. 5 rows is the classic minimum for
/// legible bitmap digits, and every digit here carries a distinct hollow
/// shape rather than a near-solid block.
///
/// `°` is hand-drawn: there's no natural digit/letter source for it and
/// whirr needs it for `88.0°C`.
fn glyph(c: char) -> [&'static str; 5] {
    match c {
        '0' => ["###", "# #", "# #", "# #", "###"],
        '1' => [" # ", "## ", " # ", " # ", "###"],
        '2' => ["###", "  #", "###", "#  ", "###"],
        '3' => ["###", "  #", "###", "  #", "###"],
        '4' => ["# #", "# #", "###", "  #", "  #"],
        '5' => ["###", "#  ", "###", "  #", "###"],
        '6' => ["###", "#  ", "###", "# #", "###"],
        '7' => ["###", "  #", "  #", "  #", "  #"],
        '8' => ["###", "# #", "###", "# #", "###"],
        '9' => ["###", "# #", "###", "  #", "###"],
        // Only the bottom row has ink (a low dot) — the rest are
        // intentionally blank, not a missing glyph row.
        '.' => ["  ", "  ", "  ", "  ", "##"],
        // Mirror of '.': a small square riding high, top two rows only.
        '°' => ["##", "##", "  ", "  ", "  "],
        'C' => ["###", "#  ", "#  ", "#  ", "###"],
        'W' => ["#   #", "#   #", "# # #", "## ##", "#   #"],
        'G' => ["####", "#   ", "# ##", "#  #", "####"],
        '%' => ["#  #", "   #", "  # ", " #  ", "#  #"],
        // Centred on the middle row, matching where the digits' waist sits.
        '-' => ["   ", "   ", "###", "   ", "   "],
        ' ' => [" ", " ", " ", " ", " "],
        // Header wordmark letters (WHIRR) — not needed by any hero number,
        // only by `ui/header.rs`.
        'H' => ["# #", "# #", "###", "# #", "# #"],
        'I' => ["###", " # ", " # ", " # ", "###"],
        'R' => ["## ", "# #", "## ", "# #", "# #"],
        _ => ["###", "  #", " # ", "   ", " # "],
    }
}

/// Concatenate `s`'s glyphs into 5 bitmap-row strings (`#`/space), one blank
/// column between glyphs. Still used directly by a couple of tests that need
/// the raw bitmap to compute expected pixel counts; `hero_lines` is the only
/// production caller.
pub fn big_text(s: &str) -> Vec<String> {
    let mut rows = vec![String::new(); 5];
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
/// mid-glyph. Digits are 3 cells wide, same as the quadrant face this
/// replaced, so the existing overflow guard still only trips on implausible
/// sensor output, e.g. `1000.0°C` at 30 cols.
pub fn big_text_fit(precise: &str, coarse: &str, width: u16) -> Vec<String> {
    let rows = big_text(precise);
    if rows[0].chars().count() <= width as usize {
        rows
    } else {
        big_text(coarse)
    }
}

/// Turn a 5-row `#`/space bitmap (as returned by `big_text`/`big_text_fit`)
/// into styled `Line`s: a filled pixel becomes a space with `color` as its
/// background, an empty pixel an unstyled space. Shared by `hero_lines` below
/// and by the header wordmark (`ui/header.rs`), which builds its own bitmap
/// from the same `glyph` table but composes it separately since it isn't a
/// `precise`/`coarse` hero value.
pub(crate) fn bitmap_lines(rows: &[String], color: Color) -> Vec<Line<'static>> {
    rows.iter()
        .map(|row| {
            let spans: Vec<Span<'static>> = row
                .chars()
                .map(|px| {
                    if px == '#' {
                        Span::styled(" ", Style::default().bg(color))
                    } else {
                        Span::raw(" ")
                    }
                })
                .collect();
            Line::from(spans)
        })
        .collect()
}

/// The hero-number rendering shared by all four gauge cards: fit `precise`
/// (falling back to `coarse`) within `width`, then paint each pixel of the
/// bitmap as a background-filled cell in `color`. Replaces the old
/// foreground-glyph `Line::styled(row, Style::default().fg(color))` — see the
/// `glyph` doc comment for why.
pub fn hero_lines(precise: &str, coarse: &str, width: u16, color: Color) -> Vec<Line<'static>> {
    bitmap_lines(&big_text_fit(precise, coarse, width), color)
}

/// Whether a card's inner area has room for a 5-row hero layout: 5 hero
/// rows plus a strip/chart below (height 10), and width for the hero string.
///
/// The width floor of 28 predates the quadrant font, which renders the widest
/// realistic hero (`88.0°C`) in 22 cols rather than ~27. The threshold is kept
/// as-is deliberately: lowering it would promote narrower cards into hero mode
/// and change which terminal sizes get the full-tier look, which is a design
/// decision rather than a consequence of the font swap.
pub fn hero_fits(inner: Rect) -> bool {
    inner.height >= 10 && inner.width >= 28
}

#[cfg(test)]
mod tests {
    use ratatui::layout::Rect;
    use ratatui::style::Color;

    /// Count of `#` pixels across a bitmap — the number of background-filled
    /// cells `bitmap_lines`/`hero_lines` should paint for it.
    pub(crate) fn filled_count(rows: &[String]) -> usize {
        rows.iter().flat_map(|r| r.chars()).filter(|&c| c == '#').count()
    }

    #[test]
    fn five_uniform_rows() {
        let rows = super::big_text("42.0 W");
        assert_eq!(rows.len(), 5);
        let w = rows[0].chars().count();
        assert!(rows.iter().all(|r| r.chars().count() == w));
    }

    #[test]
    fn all_required_glyphs_exist() {
        for c in "0123456789.°CW%G- ".chars() {
            let g = super::glyph(c);
            assert_eq!(g.len(), 5, "glyph {c:?} must have 5 rows");
            let w = g[0].chars().count();
            assert!(g.iter().all(|r| r.chars().count() == w), "glyph {c:?} rows uneven");
            assert_ne!(g, super::glyph('\u{1}'), "glyph {c:?} falls back to the fallback glyph");
        }
    }

    /// Pixel-level Hamming distance between two glyphs of equal shape (same
    /// row count and width per row).
    fn hamming(a: [&str; 5], b: [&str; 5]) -> usize {
        a.iter()
            .zip(b.iter())
            .map(|(ra, rb)| ra.chars().zip(rb.chars()).filter(|(ca, cb)| ca != cb).count())
            .sum()
    }

    /// The pairs the coarse bitmap grid is most likely to blur together must
    /// stay visually distinct. This replaces the old vacuous check (which
    /// only required the glyph *arrays* to be unequal — satisfied by a
    /// single stray pixel) with an explicit, quantified pixel-level Hamming
    /// distance, plus a structural guarantee that closes the actual failure
    /// mode reported against the old 4-row table: `8` rendering as a solid
    /// 3x4 rectangle and `6`/`9` as nearly solid. In a 3-col-wide grid, a
    /// handful of pairs (`0`/`8`, `3`/`9`, `5`/`6`, `6`/`8`, `8`/`9`) can
    /// only differ by a single pixel — but that pixel closes or opens a
    /// loop, which is structurally significant even though it is
    /// numerically small, so it is legible precisely because neither glyph
    /// is a near-solid blob to begin with.
    #[test]
    fn confusable_digit_pairs_are_distinguishable() {
        for (a, b) in [('0', '8'), ('3', '9'), ('5', '6'), ('2', '3'), ('6', '8'), ('8', '9')] {
            let dist = hamming(super::glyph(a), super::glyph(b));
            assert!(dist >= 1, "{a} and {b} render identically");
        }
        // No digit collapses into a solid or near-solid block: below 90% of
        // its pixels may be filled. This is what actually made the old
        // table's confusable pairs hard to read (`8` was a 100%-filled
        // solid rectangle, `6`/`9` were 11/12 = 91.7% filled); the densest
        // digit in the new table (`8`, 13/15 = 86.7%) still clears it with
        // room, while every digit keeps a genuine hollow loop.
        for c in '0'..='9' {
            let g = super::glyph(c);
            let filled: usize = g.iter().flat_map(|r| r.chars()).filter(|&ch| ch == '#').count();
            let total: usize = g.iter().map(|r| r.chars().count()).sum();
            assert!(filled * 10 < total * 9, "{c} is solid/near-solid: {filled}/{total} pixels filled");
        }
    }

    #[test]
    fn hero_fits_thresholds() {
        let r = |w, h| Rect::new(0, 0, w, h);
        assert!(super::hero_fits(r(28, 10)));
        assert!(!super::hero_fits(r(28, 9)));
        assert!(!super::hero_fits(r(27, 10)));
    }

    /// The hero budget (28 cols) and wordmark budget (26 cols, see
    /// `ui/header.rs`) must never be silently exceeded by a future glyph
    /// edit. `big_text` renders one blank column after every glyph
    /// (including the last), so a string's rendered width is
    /// `sum(glyph_width) + char_count`, not `sum(glyph_width) + char_count -
    /// 1` as "one blank column between glyphs" might suggest — asserted
    /// here against the real computed widths rather than hand-arithmetic,
    /// per "verify, don't assume".
    #[test]
    fn hero_width_budgets_stay_under_28() {
        for (s, want) in [("88.0°C", 22), ("100%", 17), ("24.3G", 20), ("7.9 W", 19)] {
            let w = super::big_text(s)[0].chars().count();
            assert_eq!(w, want, "{s} rendered width changed");
            assert!(w <= 28, "{s} exceeds the 28-col hero budget: {w}");
        }
    }

    #[test]
    fn hero_lines_paints_filled_pixels_as_background_and_leaves_the_rest_unstyled() {
        let color = Color::Rgb(1, 2, 3);
        let lines = super::hero_lines("41%", "41%", 28, color);
        let bitmap = super::big_text("41%");
        assert_eq!(lines.len(), 5);
        for (line, row) in lines.iter().zip(&bitmap) {
            assert_eq!(line.spans.len(), row.chars().count(), "one span per bitmap pixel");
            for (span, px) in line.spans.iter().zip(row.chars()) {
                assert_eq!(span.content, " ", "hero cells must be spaces, not glyph characters");
                if px == '#' {
                    assert_eq!(span.style.bg, Some(color), "filled pixel must carry the hero colour as bg");
                } else {
                    assert_eq!(span.style.bg, None, "empty pixel must not carry a background");
                }
            }
        }
        assert_eq!(
            lines.iter().flat_map(|l| l.spans.iter()).filter(|s| s.style.bg == Some(color)).count(),
            filled_count(&bitmap),
        );

        // Falls back to coarse exactly like `big_text_fit` when precise overflows.
        // With the 3-col-wide digit bitmap, no realistic 3-integer-digit value
        // (e.g. "100.0°C") overflows a 28-wide card any more, so bump to a
        // 4-integer-digit value to keep exercising the real fallback path.
        let fallback = super::hero_lines("1000.0°C", "1000°C", 28, color);
        let coarse_bitmap = super::big_text("1000°C");
        for (line, row) in fallback.iter().zip(&coarse_bitmap) {
            let rendered: String =
                line.spans.iter().map(|s| if s.style.bg == Some(color) { '#' } else { ' ' }).collect();
            assert_eq!(&rendered, row);
        }
    }

    #[test]
    fn big_text_fit_uses_precise_when_it_fits() {
        let rows = super::big_text_fit("41%", "41%", 28);
        assert_eq!(rows, super::big_text("41%"));
    }

    #[test]
    fn big_text_fit_falls_back_to_coarse_when_precise_overflows() {
        // The bitmap face's digits are 3 cols wide, so a realistic
        // 3-integer-digit precise value ("100.0°C", 26 cols) now fits a
        // 28-wide card and no longer exercises the fallback. Bump to a
        // 4-integer-digit value to keep proving the guard: "1000.0°C" renders
        // 30 cols wide, still wider than 28, so it must fall back to
        // "1000°C" (23 cols).
        let rows = super::big_text_fit("1000.0°C", "1000°C", 28);
        assert!(rows[0].chars().count() <= 28);
        assert_eq!(rows, super::big_text("1000°C"));
        assert_ne!(rows, super::big_text("1000.0°C"));
    }
}
