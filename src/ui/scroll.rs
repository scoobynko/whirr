//! Where a scrollable card's visible window starts.
//!
//! Every card here follows the same rule, and it is the rule rather than the
//! arithmetic that matters: **an unfocused card never scrolls.** `App` keeps
//! one cursor per panel, so a card that isn't focused has no cursor to follow
//! at all — scrolling it would move rows the user is holding as a reference
//! because they moved a *different* card's cursor. Passing `cursor: None`
//! rather than a bare index is what makes that unexpressible at the call site.

/// First visible row for a card whose rows are all one line tall.
pub fn offset(height: usize, cursor: Option<usize>) -> usize {
    match cursor {
        Some(c) => c.saturating_sub(height.saturating_sub(1)),
        None => 0,
    }
}

/// Same rule for a card whose rows are *not* all one line tall — the grouped
/// ports card charges an extra line for each group header it emits, so how far
/// to scroll can't be computed from indices alone.
///
/// `fits(from)` says whether the window `from..=cursor` fits the card. The
/// search walks down from the top, so the card scrolls the least it can, and
/// stops at `cursor` itself: a window holding only the cursor's own row is the
/// smallest one that can exist, so the search always terminates whether or not
/// that row actually fits.
pub fn offset_while(cursor: Option<usize>, fits: impl Fn(usize) -> bool) -> usize {
    let Some(cursor) = cursor else { return 0 };
    (0..cursor).find(|&from| fits(from)).unwrap_or(cursor)
}

#[cfg(test)]
mod tests {
    use super::{offset, offset_while};

    #[test]
    fn an_unfocused_card_never_scrolls() {
        // The bug this rule exists for: another panel's cursor sitting at 9
        // used to scroll this card even though it wasn't focused.
        assert_eq!(offset(1, None), 0);
        assert_eq!(offset(3, None), 0);
        assert_eq!(offset_while(None, |_| false), 0);
    }

    #[test]
    fn a_cursor_already_on_screen_does_not_scroll() {
        for cursor in 0..5 {
            assert_eq!(offset(5, Some(cursor)), 0, "cursor {cursor} fits a 5-row card");
        }
    }

    #[test]
    fn scrolling_is_the_minimum_that_keeps_the_cursor_on_screen() {
        assert_eq!(offset(5, Some(5)), 1);
        assert_eq!(offset(5, Some(9)), 5);
        // A zero-height card can't show anything; it must not underflow.
        assert_eq!(offset(0, Some(3)), 3);
    }

    #[test]
    fn variable_cost_search_takes_the_topmost_window_that_fits() {
        // Pretend rows 0..=cursor only fit once we start at 2 or later.
        assert_eq!(offset_while(Some(6), |from| from >= 2), 2);
        assert_eq!(offset_while(Some(6), |_| true), 0);
    }

    #[test]
    fn variable_cost_search_terminates_when_nothing_fits() {
        // Even a window holding only the cursor's row is reported, rather than
        // the search walking past the cursor looking for one that never exists.
        assert_eq!(offset_while(Some(6), |_| false), 6);
        assert_eq!(offset_while(Some(0), |_| false), 0);
    }
}
