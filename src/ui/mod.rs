// font::big_text and most of theme:: are consumed by panels arriving in
// Tasks 14-20; header/draw are wired in now, so only those two stay allowed.
#[allow(dead_code)]
pub mod font;
pub mod header;
#[allow(dead_code)]
pub mod theme;

use ratatui::prelude::*;

use crate::app::App;

pub fn draw(f: &mut Frame, app: &App) {
    let rows = Layout::vertical([Constraint::Length(3), Constraint::Min(0)]).split(f.area());
    header::render(f, rows[0], app);
    // body panels arrive in Tasks 14-20
}
