//! Dialogs drawn over the whole frame.
//!
//! A dialog is what the app already *behaves* like when one is open: while
//! `pending_kill` is set, `App::on_key` swallows every key and returns early.
//! Before this module that state rendered as a single line of red text inside
//! the Processes card — which meant a confirmation raised from the localhost
//! card appeared in a different panel, above the one being acted on.
//!
//! Everything here is generic over its content so the port picker and the
//! settings dialog can reuse it rather than inventing a second look.

use ratatui::prelude::*;
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph};

use super::theme;

/// One row of padding above and below the content, plus the two borders.
const CHROME_ROWS: u16 = 4;
/// Two columns of padding either side, plus the two borders.
const CHROME_COLS: u16 = 6;

/// A `w` x `h` rect centred in `area`, clamped so it always fits — a dialog
/// that would overflow a small terminal is squeezed rather than clipped, so
/// `whirr` at 48x14 still shows something answerable.
pub fn centred(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect { x: area.x + (area.width - w) / 2, y: area.y + (area.height - h) / 2, width: w, height: h }
}

/// Draw `lines` in a bordered, centred box over whatever is beneath it.
///
/// `accent` colours the border and title: the caller decides how alarming the
/// dialog looks, because "confirm a kill" and "pick a port" should not.
pub fn render(f: &mut Frame, area: Rect, title: &str, lines: Vec<Line>, accent: Color) {
    let widest = lines.iter().map(|l| l.width()).max().unwrap_or(0);
    // The title sits in the top border, so a long title widens the box too.
    let content = widest.max(title.chars().count() + 2) as u16;
    let rect = centred(area, content + CHROME_COLS, lines.len() as u16 + CHROME_ROWS);

    // `Clear` is what makes this a dialog rather than an overlay: without it
    // the cards underneath show through wherever the box doesn't paint.
    f.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .style(Style::default().bg(theme::BG_MODAL))
        .title(format!(" {title} "))
        .title_style(Style::default().fg(accent).bold());
    let inner = block.inner(rect);
    f.render_widget(block, rect);

    let body = Rect { y: inner.y + 1, height: inner.height.saturating_sub(1), ..inner };
    f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), body);
}

#[cfg(test)]
mod tests {
    use super::centred;
    use ratatui::layout::Rect;

    #[test]
    fn a_dialog_is_centred_in_its_area() {
        let r = centred(Rect::new(0, 0, 100, 40), 20, 6);
        assert_eq!((r.x, r.y, r.width, r.height), (40, 17, 20, 6));
    }

    #[test]
    fn a_dialog_too_big_for_the_terminal_is_squeezed_not_clipped() {
        // 20x5 is smaller than the box a confirmation wants. It must still
        // land inside the frame, or it renders off-screen and the user is
        // stuck at a prompt they cannot see.
        let area = Rect::new(0, 0, 20, 5);
        let r = centred(area, 60, 12);
        assert_eq!((r.width, r.height), (20, 5));
        assert!(r.x + r.width <= area.width && r.y + r.height <= area.height);
    }
}
