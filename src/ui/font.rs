use ratatui::layout::Rect;

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
        '.' => ["  ", "  ", "  ", "▄ "],
        '°' => ["▄▀▄", "▀▄▀", "   ", "   "],
        'C' => ["▄▀▀▄", "█   ", "█   ", "▀▄▄▀"],
        'W' => ["█   █", "█   █", "█ ▄ █", "▀▄▀▄▀"],
        '%' => ["█  ▄▀", "  ▄▀ ", " ▄▀  ", "▀▀  █"],
        'G' => ["▄▀▀▄", "█   ", "█ ▀█", "▀▄▄▀"],
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

/// Whether a card's inner area has room for a 4-row hero layout: 4 hero rows
/// + at least a strip/chart below (height 9) and the widest hero string
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
        for c in "0123456789.°CW%G ".chars() {
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
}
