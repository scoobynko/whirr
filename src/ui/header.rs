use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use super::theme;
use crate::app::App;
use crate::units::fmt_duration;

// Compact-tier wordmark (3 rows) — kept verbatim for small terminals.
const LOGO: [&str; 3] = [
    "█ █ █ █ █ █ █▀█ █▀█",
    "█ █ █ █▀█ █ █▀▄ █▀▄",
    "▀▄▀▄▀ █ █ █ █ █ █ █",
];

// Full-tier wordmark: W H I R R in the same 4-row tall-rounded style as
// the hero font (ui/font.rs).
const LOGO4: [&str; 4] = [
    "█   █ █  █ ▄█▄ █▀▀▄ █▀▀▄",
    "█   █ █▄▄█  █  █▄▄▀ █▄▄▀",
    "█ ▄ █ █  █  █  █ ▀▄ █ ▀▄",
    "▀▄▀▄▀ █  █ ▄█▄ █  █ █  █",
];

// Compact-tier 2-arm fan (4 frames) — kept verbatim.
const FAN_FRAMES: [[&str; 3]; 4] = [
    ["  │  ", "  ✻  ", "  │  "],
    ["   ╱ ", "  ✻  ", " ╱   "],
    ["     ", "──✻──", "     "],
    [" ╲   ", "  ✻  ", "   ╲ "],
];

// Full-tier star fan: a windmill flipping between + and × states, 4 rows
// tall to match the hero font/logo height. Blades are double-asterisk
// strokes (✳✳ pairs; horizontal arms are 2 rows thick instead, countering
// the ~2:1 cell aspect) like the e2b.dev asterisk. The flip advances every
// FAN_FLIP_TICKS ticks — a 4-arm wheel turning 45° per step shows exactly
// these two states. Single brand color.
const FAN_FLIP_TICKS: usize = 4;
const WINDMILL: [[&str; 4]; 2] = [
    [
        "    ✳✳    ",
        "✳ ✳ ✳✳ ✳ ✳",
        "✳ ✳ ✳✳ ✳ ✳",
        "    ✳✳    ",
    ],
    [
        "✳✳      ✳✳",
        " ✳✳ ✳✳ ✳✳ ",
        " ✳✳ ✳✳ ✳✳ ",
        "✳✳      ✳✳",
    ],
];

pub fn render(f: &mut Frame, area: Rect, app: &App) {
    // Full tier needs 7 rows: 1 (top pad) + 5 (band) + 1 (bottom pad).
    // Anything shorter falls back to compact.
    if area.height >= 7 {
        render_full(f, area, app);
    } else {
        render_compact(f, area, app);
    }
}

fn render_full(f: &mut Frame, area: Rect, app: &App) {
    // Breathing room: one blank row above and below the 5-row content band.
    let bands =
        Layout::vertical([Constraint::Length(1), Constraint::Length(5), Constraint::Min(0)])
            .split(area);
    let band = bands[1];
    let cols = Layout::horizontal([
        Constraint::Length(26), // logo
        Constraint::Length(11), // star fan
        Constraint::Min(0),     // ambient facts
    ])
    .split(band);

    let logo_lines: Vec<Line> = LOGO4
        .iter()
        .map(|l| Line::styled(*l, Style::default().fg(theme::ACCENT).bold()))
        .collect();
    f.render_widget(Paragraph::new(logo_lines), cols[0]);

    if !app.no_fan {
        render_star_fan(f, cols[1], app.fan_frame);
    }

    // Facts sit one row down so their block centers against the fan hub.
    let facts_area = Rect {
        y: band.y + 1,
        height: band.height.saturating_sub(1).min(3),
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
        // fan_frame advances mod 8 at double rate; halve it here so the
        // 4-frame compact fan keeps its original perceived speed.
        let frame = FAN_FRAMES[(app.fan_frame / 2) % 4];
        let fan_lines: Vec<Line> = frame
            .iter()
            .map(|l| Line::styled(*l, Style::default().fg(theme::DIM)))
            .collect();
        f.render_widget(Paragraph::new(fan_lines), cols[1]);
    }

    f.render_widget(facts_paragraph(app), cols[2]);
}

fn render_star_fan(f: &mut Frame, area: Rect, frame: usize) {
    let state = WINDMILL[(frame / FAN_FLIP_TICKS) % 2];
    let lines: Vec<Line> = state
        .iter()
        .map(|l| Line::styled(*l, Style::default().fg(theme::ACCENT)))
        .collect();
    f.render_widget(Paragraph::new(lines), area);
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

    #[test]
    fn windmill_states_are_four_rows_matching_the_font() {
        // Both states carry a center hub; the x state's blades plus hub
        // total 20 cells, the + state's 16.
        for (state, expected_stars) in super::WINDMILL.iter().zip([16, 20]) {
            assert_eq!(state.len(), 4, "fan must match the 4-row font height");
            assert!(state.iter().all(|r| r.chars().count() == 10));
            let stars: usize = state.iter().map(|r| r.matches("✳").count()).sum();
            assert_eq!(stars, expected_stars, "double-asterisk blades with hub");
        }
    }

    #[test]
    fn windmill_flips_every_fourth_tick() {
        let draw_frame = |f: usize| -> String {
            let mut t = Terminal::new(TestBackend::new(80, 7)).unwrap();
            let mut app = App::demo();
            app.fan_frame = f;
            t.draw(|fr| super::render(fr, fr.area(), &app)).unwrap();
            t.backend().buffer().content().iter().map(|c| c.symbol()).collect()
        };
        assert_eq!(draw_frame(0), draw_frame(3), "state must hold for 4 ticks");
        assert_ne!(draw_frame(0), draw_frame(4), "state must flip on the 4th tick");
        assert_ne!(draw_frame(3), draw_frame(4), "flip boundary at frame 4");
    }

    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    use crate::app::App;

    fn draw_header(w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        let app = App::demo();
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        t.backend().buffer().content().iter().map(|c| c.symbol()).collect()
    }

    #[test]
    fn full_tier_needs_seven_rows_for_unclipped_star_fan() {
        let full = draw_header(80, 7);
        assert!(full.contains("✳"), "star fan missing at height 7");
        for h in [5, 6] {
            let short = draw_header(80, h);
            assert!(!short.contains("✳"), "height {h} must fall back to compact");
        }
    }

    #[test]
    fn star_fan_uses_only_brand_accent() {
        let mut t = Terminal::new(TestBackend::new(80, 7)).unwrap();
        let app = App::demo();
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        let buf = t.backend().buffer().clone();
        for c in buf.content().iter().filter(|c| c.symbol() == "✳") {
            assert_eq!(
                c.style().fg,
                Some(crate::ui::theme::ACCENT),
                "star cells must use the brand accent color"
            );
        }
    }
}
