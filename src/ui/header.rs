use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use super::burst;
use super::font;
use super::theme;
use crate::app::App;
use crate::units::fmt_duration;

// Compact tier used to carry its own 3-row wordmark drawn with foreground
// block/half-block glyphs (`█ █ █ █▀█ █▀▄`). Terminal.app draws those from the
// font's outlines rather than as native rectangles, so the letters seamed and
// broke up at small sizes — the same defect `ui/font.rs` documents for the
// hero digits. Both tiers now share one background-filled bitmap face, which
// is why the compact header band is 5 rows rather than 3.

// Wordmark: W H I R R, built from the same background-filled pixel
// bitmap as the hero font (`ui/font.rs::glyph`) rather than drawn with
// foreground block characters — see that module's doc comment for why
// (Terminal.app seams foreground block glyphs but never a cell background).
// Shared by both tiers: the compact header band is sized to this face's 5
// rows rather than carrying a separate, smaller one.
fn logo4_lines(theme: &theme::Theme) -> Vec<Line<'static>> {
    font::bitmap_lines(&font::big_text("WHIRR"), theme.accent)
}

// The fan is the braille burst at every size — a radial spray of
// counter-rotating ray halves (see `ui/burst.rs`), which rasterizes to
// whatever rect it is handed. The compact tier used to substitute four
// hand-drawn `✻` frames; they were the last of the pre-refresh assets and
// read as a generic spinner rather than as whirr's fan.
const FAN_COLS: u16 = 19;
const FAN_ROWS: u16 = 7;
// Below 11x5 the burst degrades into an undifferentiated blob — too few
// braille dots to separate ten rays from the hub — so the compact tier gets
// the smallest size that still reads as a burst.
const COMPACT_FAN_COLS: u16 = 11;
const COMPACT_FAN_ROWS: u16 = 5;
/// Columns the compact wordmark needs: the shared bitmap face is 22 wide
/// (see `logo4_is_five_uniform_rows_within_budget`), plus two of padding.
const COMPACT_LOGO_COLS: u16 = 24;

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    // Full tier needs 9 rows: 1 (top pad) + 7 (band, sized to the fan) +
    // 1 (bottom pad). Anything shorter falls back to compact.
    if area.height >= 9 {
        render_full(f, area, app);
    } else {
        render_compact(f, area, app);
    }
}

fn render_full(f: &mut Frame, area: Rect, app: &App) {
    let cols = Layout::horizontal([
        Constraint::Length(26),       // logo
        Constraint::Length(FAN_COLS), // burst fan
        Constraint::Min(0),           // ambient facts
    ])
    .split(area);

    let logo_area = Rect { y: area.y + 2, height: area.height.saturating_sub(2).min(5), ..cols[0] };
    f.render_widget(Paragraph::new(logo4_lines(&app.theme)), logo_area);

    if !app.no_fan {
        // The burst sits 19x7 centred in the 9-row band — a blank row above
        // and below. It scales to whatever rect it is given, so the size lives
        // here rather than in the rasterizer.
        let fan = Rect {
            y: area.y + area.height.saturating_sub(FAN_ROWS) / 2,
            height: FAN_ROWS.min(area.height),
            ..cols[1]
        };
        f.render_widget(Paragraph::new(burst::render(fan.width, fan.height, &app.theme, app.fan_angle_deg)), fan);
    }

    let facts_area = Rect {
        y: area.y + 3,
        height: area.height.saturating_sub(3).min(3),
        ..cols[2]
    };
    f.render_widget(facts_paragraph(app), facts_area);
}

fn render_compact(f: &mut Frame, area: Rect, app: &App) {
    // At very small terminal heights the outer layout can squeeze this band
    // below the bitmap face's 5 rows. A clipped bitmap reads as a broken
    // wordmark, so drop to plain styled text instead — it needs one row, and
    // being ordinary text it has no block glyphs to seam.
    if area.height < COMPACT_FAN_ROWS {
        let cols = Layout::horizontal([Constraint::Length(8), Constraint::Min(0)]).split(area);
        f.render_widget(
            Paragraph::new(Line::styled("whirr", Style::default().fg(app.theme.accent).bold())),
            cols[0],
        );
        f.render_widget(facts_paragraph(app), cols[1]);
        return;
    }
    // The fan is dropped rather than shrunk further when the terminal is too
    // narrow to carry both it and the wordmark alongside the facts — a
    // squeezed burst reads as noise, and the facts are the useful content.
    let room_for_fan = area.width >= COMPACT_LOGO_COLS + COMPACT_FAN_COLS + 20;
    let fan_cols = if room_for_fan { COMPACT_FAN_COLS } else { 0 };
    let cols = Layout::horizontal([
        Constraint::Length(COMPACT_LOGO_COLS),
        Constraint::Length(fan_cols),
        Constraint::Min(0), // ambient facts
    ])
    .split(area);

    // Same bitmap face as the full tier, painted as cell backgrounds.
    f.render_widget(Paragraph::new(logo4_lines(&app.theme)), cols[0]);

    if !app.no_fan && room_for_fan {
        let fan = Rect {
            height: COMPACT_FAN_ROWS.min(area.height),
            ..cols[1]
        };
        f.render_widget(Paragraph::new(burst::render(fan.width, fan.height, &app.theme, app.fan_angle_deg)), fan);
    }

    f.render_widget(facts_paragraph(app), cols[2]);
}

fn facts_paragraph(app: &App) -> Paragraph<'_> {
    let (uptime, load) = (
        app.medium.as_ref().map_or(0, |m| m.uptime_secs),
        app.fast.as_ref().map_or(0.0, |f| f.load_avg),
    );
    let facts = vec![
        Line::from(format!(
            "{} · macOS {}",
            app.statics.chip, app.statics.os_version
        )),
        Line::from(format!("up {} · load {:.2}", fmt_duration(uptime), load)),
        // Costs no layout: this row was already reserved and rendered blank.
        //
        // Without it the version is only ever visible when it is *wrong* —
        // the footer's update notice appears, and otherwise nothing says what
        // you are running. "No notice" would then mean both "you are current"
        // and "the check is switched off", which are not the same thing. It
        // also saves a trip to `whirr --version` when reporting a bug.
        //
        // `v` prefixed to match the git tags and GitHub releases, rather than
        // the bare number `--version` prints.
        Line::from(format!("v{}", env!("CARGO_PKG_VERSION"))),
    ];
    Paragraph::new(facts)
        .style(Style::default().fg(app.theme.dim))
        .alignment(Alignment::Right)
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::app::App;


    #[test]
    fn logo4_is_five_uniform_rows_within_budget() {
        let rows = crate::ui::font::big_text("WHIRR");
        assert_eq!(rows.len(), 5);
        let w = rows[0].chars().count();
        assert!(rows.iter().all(|r| r.chars().count() == w));
        // See `ui/font.rs`'s `hero_width_budgets_stay_under_28` for why the
        // real rendered width (22) is one wider than the naive "one blank
        // column between glyphs" arithmetic (21) would suggest: `big_text`
        // also trails a blank column after the final glyph.
        assert_eq!(w, 22);
        assert!(w <= 26);
    }

    /// The wordmark used to be drawn with foreground quadrant/block
    /// characters, which seam in Terminal.app (see `ui/font.rs`'s doc
    /// comment). It must now be background-filled cells: every rendered
    /// symbol in the logo area is a plain space, and the cells that should
    /// be "ink" carry the accent colour as `bg` — sampled here directly from
    /// the buffer rather than from rendered text, since a text dump can't
    /// show a background colour.
    #[test]
    fn logo4_is_painted_as_background_fills_not_foreground_glyphs() {
        let mut t = Terminal::new(TestBackend::new(80, 9)).unwrap();
        let app = App::demo();
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        let buf = t.backend().buffer().clone();

        let logo_symbols: String = (0..26).flat_map(|x| (2..7).map(move |y| (x, y))).map(|(x, y)| buf[(x, y)].symbol().to_string()).collect();
        assert!(
            logo_symbols.chars().all(|c| c == ' '),
            "logo area must render as spaces with a background colour, found {logo_symbols:?}"
        );

        let filled_cells = buf
            .content()
            .iter()
            .filter(|cell| cell.style().bg == Some(app.theme.accent))
            .count();
        let bitmap = crate::ui::font::big_text("WHIRR");
        let expected: usize = bitmap.iter().flat_map(|r| r.chars()).filter(|&c| c == '#').count();
        assert_eq!(filled_cells, expected, "background-filled cell count must match the WHIRR bitmap");
    }

    fn draw_header(w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        let app = App::demo();
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        t.backend().buffer().content().iter().map(|c| c.symbol()).collect()
    }

    /// Any braille glyph with at least one dot. `U+2800` is the blank and must
    /// never be emitted — empty cells are plain spaces.
    fn has_braille(s: &str) -> bool {
        s.chars().any(|c| ('\u{2801}'..='\u{28FF}').contains(&c))
    }

    /// Both tiers draw the same braille burst, just at different sizes. The
    /// compact tier used to substitute four hand-drawn `✻` frames — the last
    /// of the pre-refresh assets — so this pins that they are gone and that
    /// the smaller header still gets a real burst rather than nothing.
    #[test]
    fn both_tiers_draw_the_burst_at_their_own_size() {
        let full = draw_header(80, 9);
        let compact = draw_header(80, 5);
        assert!(has_braille(&full), "burst missing from the full header");
        assert!(has_braille(&compact), "burst missing from the compact header");
        assert!(!compact.contains('✻'), "hand-drawn compact fan should be gone");
        let ink = |s: &str| s.chars().filter(|&c| ('\u{2801}'..='\u{28FF}').contains(&c)).count();
        assert!(
            ink(&full) > ink(&compact),
            "the full tier's 19x7 burst should carry more ink than the compact 11x5 one"
        );
    }

    /// The compact wordmark must be background-filled cells too, not the old
    /// foreground block face — that face is exactly what broke up in
    /// Terminal.app at small sizes.
    #[test]
    fn compact_wordmark_is_also_painted_as_background_fills() {
        let mut t = Terminal::new(TestBackend::new(80, 5)).unwrap();
        let app = App::demo();
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        let buf = t.backend().buffer().clone();
        let filled = buf.content().iter().filter(|c| c.style().bg == Some(app.theme.accent)).count();
        let expected: usize = crate::ui::font::big_text("WHIRR")
            .iter()
            .flat_map(|r| r.chars())
            .filter(|&c| c == '#')
            .count();
        assert_eq!(filled, expected, "compact wordmark bitmap pixel count mismatch");
    }

    #[test]
    fn burst_is_centred_in_the_nine_row_band() {
        let mut t = Terminal::new(TestBackend::new(80, 9)).unwrap();
        let app = App::demo();
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        let buf = t.backend().buffer().clone();
        let row = |y: u16| -> String { (0..80).map(|x| buf[(x, y)].symbol()).collect() };
        // 19x7 centred in 9 rows: ink reaches rows 1 and 7 (the vertical ray
        // tips) and rows 0 and 8 stay clear.
        assert!(has_braille(&row(1)), "no burst ink in header row 1");
        assert!(has_braille(&row(7)), "no burst ink in header row 7");
        assert!(!has_braille(&row(0)), "burst should not reach row 0");
        assert!(!has_braille(&row(8)), "burst should not reach row 8");
    }

    #[test]
    fn burst_rotates_between_angles() {
        let draw = |deg: f32| -> String {
            let mut t = Terminal::new(TestBackend::new(80, 9)).unwrap();
            let mut app = App::demo();
            app.fan_angle_deg = deg;
            t.draw(|fr| super::render(fr, fr.area(), &app)).unwrap();
            t.backend().buffer().content().iter().map(|c| c.symbol()).collect()
        };
        let base = draw(0.0);
        for deg in [9.0, 18.0, 27.0] {
            assert_ne!(base, draw(deg), "{deg}° renders identically to 0°");
        }
    }

    #[test]
    fn burst_uses_only_blends_of_the_two_brand_tones() {
        let mut t = Terminal::new(TestBackend::new(80, 9)).unwrap();
        let app = App::demo();
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        let buf = t.backend().buffer().clone();
        let (mut saw_text, mut saw_accent) = (false, false);
        for cell in buf.content() {
            let c = cell.symbol().chars().next().unwrap();
            if !('\u{2801}'..='\u{28FF}').contains(&c) {
                continue;
            }
            let fg = cell.style().fg.unwrap();
            // Reuse the blend-line check from the burst module's tests.
            if crate::ui::burst::tests::is_blend_of(fg, app.theme.text) {
                saw_text = true;
            } else if crate::ui::burst::tests::is_blend_of(fg, app.theme.accent) {
                saw_accent = true;
            } else {
                panic!("off-brand colour {fg:?}");
            }
        }
        assert!(saw_text && saw_accent, "both brand tones must appear");
    }

    #[test]
    fn no_fan_leaves_the_header_free_of_braille() {
        let mut t = Terminal::new(TestBackend::new(80, 9)).unwrap();
        let mut app = App::demo();
        app.no_fan = true;
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        let s: String = t.backend().buffer().content().iter().map(|c| c.symbol()).collect();
        assert!(!has_braille(&s), "--no-fan must not draw the burst");
    }
}
