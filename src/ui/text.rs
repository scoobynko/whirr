//! Display-width text helpers shared by the row-rendering cards.
//!
//! Widths here are terminal *cells*, not chars — a CJK character costs 2
//! cells, so counting chars overstates how much room is left and lets the real
//! terminal clip content a width-aware budget thought was safe. Ratatui already
//! computes this internally; reuse it instead of pulling in a new dependency.

use ratatui::text::Span;

/// Terminal display width of `s`, in cells.
pub fn width(s: &str) -> usize {
    Span::from(s).width()
}

/// Right-pad `s` with spaces to `to` display cells. `s` must already fit within
/// `to` (e.g. via `trunc`) — this only grows short strings, it never clips.
pub fn pad(s: &str, to: usize) -> String {
    let w = width(s);
    if w >= to {
        s.to_string()
    } else {
        format!("{s}{}", " ".repeat(to - w))
    }
}

/// Clip to `max` display cells without splitting a char or a wide glyph,
/// marking the cut with an ellipsis.
pub fn trunc(s: &str, max: usize) -> String {
    if width(s) <= max {
        return s.to_string();
    }
    let budget = max.saturating_sub(1);
    let mut out = String::new();
    for ch in s.chars() {
        let mut candidate = out.clone();
        candidate.push(ch);
        if width(&candidate) > budget {
            break;
        }
        out = candidate;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::{pad, trunc, width};

    #[test]
    fn width_counts_cells_not_chars() {
        assert_eq!(width("abc"), 3);
        // 5 chars, 10 cells — the whole reason this module exists.
        assert_eq!(width("项目名称测"), 10);
    }

    #[test]
    fn pad_grows_but_never_clips() {
        assert_eq!(pad("ab", 4), "ab  ");
        assert_eq!(pad("abcd", 2), "abcd", "pad must not clip; that is trunc's job");
        // Padding is measured in cells, so a wide glyph counts double.
        assert_eq!(pad("项", 4), "项  ");
    }

    #[test]
    fn trunc_leaves_short_strings_alone() {
        assert_eq!(trunc("abc", 5), "abc");
        assert_eq!(trunc("abc", 3), "abc");
    }

    #[test]
    fn trunc_never_splits_a_wide_glyph() {
        // Budget 5 leaves 4 cells before the ellipsis: two wide chars fit, a
        // third would need a 5th cell, so it must be dropped whole.
        let out = trunc("项目名称", 5);
        assert_eq!(out, "项目…");
        assert!(width(&out) <= 5, "{out:?} overflowed its budget");
    }

    #[test]
    fn trunc_marks_every_cut_with_exactly_one_ellipsis() {
        let out = trunc("abcdefgh", 4);
        assert_eq!(out, "abc…");
        assert_eq!(out.matches('…').count(), 1);
    }
}
