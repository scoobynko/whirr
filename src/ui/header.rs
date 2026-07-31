use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use super::burst;
use super::theme;
use crate::app::App;
use crate::units::fmt_duration;

// Compact-tier wordmark (3 rows) — kept verbatim for small terminals.
const LOGO: [&str; 3] = [
    "█ █ █ █ █ █ █▀█ █▀█",
    "█ █ █ █▀█ █ █▀▄ █▀▄",
    "▀▄▀▄▀ █ █ █ █ █ █ █",
];

// Full-tier wordmark: W H I R R in the same 4-row quadrant style as the hero
// font (ui/font.rs), transcribed from FIGlet `smblock`, with a space between
// letters — kerned tight (14 cols) the strokes run together at this weight.
// The compact tier keeps its own 3-row wordmark below: this face is 4 rows and
// the compact header band is only 3.
const LOGO4: [&str; 4] = [
    "▌ ▌ ▌ ▌ ▜▘ ▛▀▖ ▛▀▖",
    "▌▖▌ ▙▄▌ ▐  ▙▄▘ ▙▄▘",
    "▙▚▌ ▌ ▌ ▐  ▌▚  ▌▚ ",
    "▘ ▘ ▘ ▘ ▀▘ ▘ ▘ ▘ ▘",
];

// Compact-tier 2-arm fan (4 frames) — kept verbatim.
const FAN_FRAMES: [[&str; 3]; 4] = [
    ["  │  ", "  ✻  ", "  │  "],
    ["   ╱ ", "  ✻  ", " ╱   "],
    ["     ", "──✻──", "     "],
    [" ╲   ", "  ✻  ", "   ╲ "],
];

// Full-tier fan: a radial burst of counter-rotating ray halves on a braille
// dot canvas — see `ui/burst.rs`.
const FAN_COLS: u16 = 19;
const FAN_ROWS: u16 = 7;

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

    let logo_area = Rect { y: area.y + 2, height: area.height.saturating_sub(2).min(4), ..cols[0] };
    let logo_lines: Vec<Line> = LOGO4
        .iter()
        .map(|l| Line::styled(*l, Style::default().fg(theme::ACCENT).bold()))
        .collect();
    f.render_widget(Paragraph::new(logo_lines), logo_area);

    if !app.no_fan {
        // The burst sits 19x7 centred in the 9-row band — a blank row above
        // and below. It scales to whatever rect it is given, so the size lives
        // here rather than in the rasterizer.
        let fan = Rect {
            y: area.y + area.height.saturating_sub(FAN_ROWS) / 2,
            height: FAN_ROWS.min(area.height),
            ..cols[1]
        };
        f.render_widget(Paragraph::new(burst::render(fan.width, fan.height, app.fan_angle_deg)), fan);
    }

    let facts_area = Rect {
        y: area.y + 3,
        height: area.height.saturating_sub(3).min(3),
        ..cols[2]
    };
    f.render_widget(facts_paragraph(app), facts_area);
}

fn render_compact(f: &mut Frame, area: Rect, app: &App) {
    let cols = Layout::horizontal([
        Constraint::Length(21), // logo
        Constraint::Length(7),  // fan
        Constraint::Min(0),     // ambient facts
    ])
    .split(area);

    let logo_lines: Vec<Line> = LOGO
        .iter()
        .map(|l| Line::styled(*l, Style::default().fg(theme::ACCENT).bold()))
        .collect();
    f.render_widget(Paragraph::new(logo_lines), cols[0]);

    if !app.no_fan {
        // The compact fan keeps its 4 hand-drawn frames; a quarter turn of the
        // burst's angle advances it by one.
        let frame = FAN_FRAMES[(app.fan_angle_deg / 90.0) as usize % 4];
        let fan_lines: Vec<Line> = frame
            .iter()
            .map(|l| Line::styled(*l, Style::default().fg(theme::DIM)))
            .collect();
        f.render_widget(Paragraph::new(fan_lines), cols[1]);
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
        Line::from(""),
    ];
    Paragraph::new(facts)
        .style(Style::default().fg(theme::DIM))
        .alignment(Alignment::Right)
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::app::App;
    use crate::ui::theme;

    #[test]
    fn logo_is_three_uniform_rows_within_budget() {
        let w = super::LOGO[0].chars().count();
        assert!(super::LOGO.iter().all(|r| r.chars().count() == w));
        assert!(w <= 21);
    }

    #[test]
    fn logo4_is_four_uniform_rows_within_budget() {
        let w = super::LOGO4[0].chars().count();
        assert_eq!(super::LOGO4.len(), 4);
        assert!(super::LOGO4.iter().all(|r| r.chars().count() == w));
        assert!(w <= 26);
    }

    #[test]
    fn fan_frames_are_uniform() {
        for frame in super::FAN_FRAMES {
            let w = frame[0].chars().count();
            assert!(frame.iter().all(|r| r.chars().count() == w));
        }
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

    #[test]
    fn full_tier_needs_nine_rows_for_the_burst() {
        assert!(has_braille(&draw_header(80, 9)), "burst missing at height 9");
        for h in [5, 6, 7, 8] {
            assert!(!has_braille(&draw_header(80, h)), "height {h} must fall back to compact");
        }
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
            if crate::ui::burst::tests::is_blend_of(fg, theme::TEXT) {
                saw_text = true;
            } else if crate::ui::burst::tests::is_blend_of(fg, theme::ACCENT) {
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
