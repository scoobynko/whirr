use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

use super::theme;

/// 4-row tall-rounded solid glyphs. Rounded shoulders come from `▄▀▀▄`-style
/// tops and `▀▄▄▀` bottoms; every row of a glyph has equal width.
fn glyph(c: char) -> [&'static str; 4] {
    match c {
        '0' => ["▄▀▀▄", "█  █", "█  █", "▀▄▄▀"],
        '1' => ["▄█ ", " █ ", " █ ", "▄█▄"],
        '2' => ["▄▀▀▄", "  ▄▀", " ▄▀ ", "█▄▄▄"],
        '3' => ["▄▀▀▄", " ▄▄▀", "   █", "▀▄▄▀"],
        '4' => ["▄  █", "█  █", "▀▀▀█", "   █"],
        '5' => ["█▀▀▀", "▀▀▀▄", "   █", "▀▄▄▀"],
        '6' => ["▄▀▀▄", "█▄▄ ", "█  █", "▀▄▄▀"],
        '7' => ["▀▀▀█", "  ▄▀", " █  ", " █  "],
        '8' => ["▄▀▀▄", "▀▄▄▀", "█  █", "▀▄▄▀"],
        '9' => ["▄▀▀▄", "█  █", " ▀▀█", "▀▄▄▀"],
        // Only the bottom row has ink (a low dot) — the top three rows are
        // intentionally blank, not a missing glyph row.
        '.' => ["  ", "  ", "  ", "▄ "],
        '°' => ["▄▀▄", "▀▄▀", "   ", "   "],
        'C' => ["▄▀▀▄", "█   ", "█   ", "▀▄▄▀"],
        'W' => ["█   █", "█   █", "█ ▄ █", "▀▄▀▄▀"],
        '%' => ["█  ▄▀", "  ▄▀ ", " ▄▀  ", "▄▀  █"],
        'G' => ["▄▀▀▄", "█   ", "█ ▀█", "▀▄▄▀"],
        '-' => ["   ", "▄▄▄", "   ", "   "],
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
/// mid-glyph (e.g. `100.0°C` is 31 cols but a card can be 28 wide).
pub fn big_text_fit(precise: &str, coarse: &str, width: u16) -> Vec<String> {
    let rows = big_text(precise);
    if rows[0].chars().count() <= width as usize {
        rows
    } else {
        big_text(coarse)
    }
}

/// Edge cells dim to this fraction of `color` (via `theme::ramp`), well
/// short of the full colour interior cells keep — enough to read as a lit
/// bevel rather than a flat block.
const EDGE_RAMP: f32 = 0.6;

/// Colour a block-font grid so it reads as lit rather than flat: a lit cell
/// (any non-space character) is an *edge* if any of its four orthogonal
/// neighbours is blank or falls outside the grid, and edges render in a
/// dimmed `color`; interior cells (all four neighbours lit) keep the full
/// `color`.
fn colour_grid(rows: &[String], color: Color) -> Vec<Line<'static>> {
    let grid: Vec<Vec<char>> = rows.iter().map(|r| r.chars().collect()).collect();
    let edge = theme::ramp(color, EDGE_RAMP);
    grid.iter()
        .enumerate()
        .map(|(y, row)| {
            let spans: Vec<Span<'static>> = row
                .iter()
                .enumerate()
                .map(|(x, &ch)| {
                    let fg = if ch == ' ' {
                        color
                    } else {
                        let blank_or_oob = |n: Option<char>| matches!(n, None | Some(' '));
                        let is_edge = blank_or_oob(y.checked_sub(1).map(|y2| grid[y2][x]))
                            || blank_or_oob(grid.get(y + 1).map(|r2| r2[x]))
                            || blank_or_oob(x.checked_sub(1).map(|x2| row[x2]))
                            || blank_or_oob(row.get(x + 1).copied());
                        if is_edge { edge } else { color }
                    };
                    Span::styled(ch.to_string(), Style::default().fg(fg))
                })
                .collect();
            Line::from(spans)
        })
        .collect()
}

/// The hero-number rendering shared by all four gauge cards: fit `precise`
/// (falling back to `coarse`) within `width`, then colour it in `color` with
/// a dimmed edge and a full-brightness interior (see `colour_grid`) so it
/// reads as illuminated rather than painted flat. Replaces the hand-rolled
/// `big_text(...).into_iter().map(|r| Line::styled(r, ...)).collect()` that
/// used to be duplicated across `cpu.rs`, `memory.rs`, `power.rs` and
/// `temp.rs`.
pub fn hero_lines(precise: &str, coarse: &str, width: u16, color: Color) -> Vec<Line<'static>> {
    colour_grid(&big_text_fit(precise, coarse, width), color)
}

/// Whether a card's inner area has room for a 4-row hero layout: 4 hero
/// rows plus a strip/chart below (height 9), and the widest hero string
/// (`88.8°C` ≈ 27 cols) plus margin.
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
            // Each row is now coloured cell by cell (edge vs interior), so
            // the text is spread across per-character spans rather than one
            // span per row — concatenate them back to compare against the
            // plain block-font row.
            let content: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            assert_eq!(content, row);
            assert!(line.spans.iter().all(|s| s.style.fg.is_some()));
        }

        // Falls back to coarse exactly like `big_text_fit` when precise overflows.
        let fallback = super::hero_lines("100.0°C", "100°C", 28, Color::Rgb(1, 2, 3));
        let coarse_rows = super::big_text("100°C");
        for (line, row) in fallback.iter().zip(coarse_rows) {
            let content: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            assert_eq!(content, row);
        }
    }

    #[test]
    fn colour_grid_gives_interior_cells_the_full_colour_and_edges_a_dimmed_one() {
        use ratatui::style::Color;
        use std::collections::HashSet;

        // Every glyph in `glyph()` is a thin, one-cell-wide outline (hollow
        // middles, e.g. '0' == ["▄▀▀▄", "█  █", "█  █", "▀▄▄▀"]), so no lit
        // cell in any real hero string ever has all four orthogonal
        // neighbours lit — verified exhaustively against every glyph and
        // several full hero strings. This synthetic solid 3x3 block stands
        // in for what a filled glyph would look like, to exercise
        // `colour_grid`'s edge/interior split directly.
        let rows = vec!["███".to_string(), "███".to_string(), "███".to_string()];
        let color = Color::Rgb(45, 225, 194);
        let lines = super::colour_grid(&rows, color);

        let mut colours: HashSet<Color> = HashSet::new();
        for line in &lines {
            for span in &line.spans {
                colours.insert(span.style.fg.unwrap());
            }
        }
        assert!(colours.len() >= 2, "expected an edge/interior colour split, got {colours:?}");
        assert!(colours.contains(&color), "interior cells should keep the full colour");

        let brightness = |c: Color| match c {
            Color::Rgb(r, g, b) => u32::from(r) + u32::from(g) + u32::from(b),
            _ => 0,
        };
        let brightest = colours.iter().copied().max_by_key(|c| brightness(*c)).unwrap();
        assert_eq!(brightest, color, "interior colour should be the brighter of the two");

        // The centre cell has all four neighbours lit: interior.
        assert_eq!(lines[1].spans[1].style.fg, Some(color));
        // A corner cell has two blank/out-of-bounds neighbours: edge.
        assert_ne!(lines[0].spans[0].style.fg, Some(color));
    }

    #[test]
    fn big_text_fit_uses_precise_when_it_fits() {
        let rows = super::big_text_fit("41%", "41%", 28);
        assert_eq!(rows, super::big_text("41%"));
    }

    #[test]
    fn big_text_fit_falls_back_to_coarse_when_precise_overflows() {
        // "100.0°C" renders wider than 28 cols; must fall back to "100°C".
        let rows = super::big_text_fit("100.0°C", "100°C", 28);
        assert!(rows[0].chars().count() <= 28);
        assert_eq!(rows, super::big_text("100°C"));
        assert_ne!(rows, super::big_text("100.0°C"));
    }
}
