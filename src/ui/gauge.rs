//! The shell the four gauge cards share.
//!
//! What is genuinely common between CPU, Temp, Power and Memory is the frame,
//! the "this sample hasn't arrived" state, and the two shapes a headline number
//! takes. What each card puts *below* its headline is not common — a chart, a
//! legend, a segmented bar — so that stays in the cards. Sharing only the shell
//! is what keeps "a card with no data looks like every other card with no data"
//! true by construction rather than by four separate reviews.

use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use super::{font, theme};

/// Draw a gauge card's border and title; returns the area inside it.
pub fn frame(f: &mut Frame, area: Rect, title: &str) -> Rect {
    let block = theme::panel_block(title, false);
    let inner = block.inner(area);
    f.render_widget(block, area);
    inner
}

/// Fill a card whose sample hasn't arrived yet.
pub fn unavailable(f: &mut Frame, inner: Rect) {
    f.render_widget(Paragraph::new("n/a").style(Style::default().fg(theme::DIM)), inner);
}

/// Rows a hero headline claims. Cards lay out around this, so it lives here
/// rather than being written as a bare `5` in four places.
pub const HERO_ROWS: u16 = 5;

/// The card's headline as a 5-row bitmap number. `precise` falls back to
/// `coarse` when the card is too narrow to render it whole — a hero value must
/// never truncate mid-glyph.
pub fn hero(f: &mut Frame, area: Rect, precise: &str, coarse: &str, color: Color) {
    f.render_widget(Paragraph::new(font::hero_lines(precise, coarse, area.width, color)), area);
}

/// The card's headline as a single bold row, for cards too short for a hero.
pub fn readout(f: &mut Frame, area: Rect, text: &str, color: Color) {
    f.render_widget(
        Paragraph::new(Span::styled(text.to_string(), Style::default().fg(color).bold())),
        area,
    );
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::layout::Rect;
    use ratatui::Terminal;

    use super::super::theme;
    use crate::app::App;

    /// A gauge card's entry point, as `ui::draw` calls it.
    type CardFn = fn(&mut ratatui::Frame, Rect, &App);

    /// Every gauge card's empty state must look the same. Drawn through the
    /// real card entry points rather than through this module, so a card that
    /// stopped using the shell would show up here.
    #[test]
    fn every_gauge_card_renders_the_same_empty_state() {
        // An App with no samples at all: fast/medium/slow are all None, so
        // each card takes its unavailable path.
        let app = App::new(false);
        let cards: [(&str, CardFn); 4] = [
            ("CPU", crate::ui::cpu::render),
            ("Temp", crate::ui::temp::render),
            ("Power", crate::ui::power::render),
            ("Memory", crate::ui::memory::render),
        ];
        for (title, render) in cards {
            let mut t = Terminal::new(TestBackend::new(40, 12)).unwrap();
            t.draw(|f| render(f, f.area(), &app)).unwrap();
            let buf = t.backend().buffer().clone();
            let text: String = buf.content().iter().map(|c| c.symbol()).collect();
            assert!(text.contains(title), "{title} card lost its title");
            assert!(text.contains("n/a"), "{title} card should say n/a with no sample");
            assert!(
                buf.content().iter().all(|c| c.style().bg != Some(theme::ACCENT)),
                "{title} card painted a hero number with no data behind it"
            );
        }
    }
}
