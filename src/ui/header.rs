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

// Full-tier star fan: an e2b.dev-style asterisk of 8 blades with 3 star
// cells each, rasterized from angle math every frame so the whole figure
// rotates continuously — 15° per tick, FAN_ROT_FRAMES ticks per
// revolution. Blades within ~30° of vertical draw ✳✳ pairs: at the
// terminal's ~2:1 cell aspect a single ✳ column looks threadbare next to
// the stretched horizontal blades. Single brand color.
const FAN_H: usize = 7;
const FAN_W: usize = 17;
const FAN_ROT_FRAMES: usize = 24;

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
    // Breathing room: one blank row above and below the 7-row content band.
    let bands =
        Layout::vertical([Constraint::Length(1), Constraint::Length(7), Constraint::Min(0)])
            .split(area);
    let band = bands[1];
    let cols = Layout::horizontal([
        Constraint::Length(26), // logo
        Constraint::Length(19), // star fan
        Constraint::Min(0),     // ambient facts
    ])
    .split(band);

    // Logo and facts center against the 7-row fan.
    let logo_area = Rect { y: band.y + 1, height: band.height.saturating_sub(1).min(4), ..cols[0] };
    let logo_lines: Vec<Line> = LOGO4
        .iter()
        .map(|l| Line::styled(*l, Style::default().fg(theme::ACCENT).bold()))
        .collect();
    f.render_widget(Paragraph::new(logo_lines), logo_area);

    if !app.no_fan {
        render_star_fan(f, cols[1], app.fan_frame);
    }

    let facts_area = Rect {
        y: band.y + 2,
        height: band.height.saturating_sub(2).min(3),
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
    use std::f32::consts::{FRAC_PI_4, TAU};
    let mut grid = [[None::<Color>; FAN_W]; FAN_H];
    let base = frame as f32 * TAU / FAN_ROT_FRAMES as f32;
    for arm in 0..8 {
        // Arms alternate the two brand tones (like e2b's black/orange);
        // the tone travels with the arm as it rotates.
        let color = if arm % 2 == 0 { theme::TEXT } else { theme::ACCENT };
        let theta = base + arm as f32 * FRAC_PI_4;
        let vertical_ish = theta.sin().abs() < 0.5;
        for r in [1.4f32, 2.3, 3.2] {
            let row = (3.0 - r * theta.cos()).round() as i32;
            // Columns stretched x2.2 against the ~2:1 cell aspect; the pair
            // shifts half a cell left so ✳✳ stays centered on the blade.
            let shift = if vertical_ish { 0.5 } else { 0.0 };
            let col = (8.0 + 2.2 * r * theta.sin() - shift).round() as i32;
            let span = if vertical_ish { 2 } else { 1 };
            for c in col..col + span {
                if (0..FAN_H as i32).contains(&row) && (0..FAN_W as i32).contains(&c) {
                    grid[row as usize][c as usize] = Some(color);
                }
            }
        }
    }
    let lines: Vec<Line> = grid
        .iter()
        .map(|row| {
            Line::from(
                row.iter()
                    .map(|cell| match cell {
                        Some(color) => Span::styled("✳", Style::default().fg(*color)),
                        None => Span::raw(" "),
                    })
                    .collect::<Vec<_>>(),
            )
        })
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
    fn star_fan_rotates_between_adjacent_frames() {
        let draw_frame = |f: usize| -> String {
            let mut t = Terminal::new(TestBackend::new(80, 9)).unwrap();
            let mut app = App::demo();
            app.fan_frame = f;
            t.draw(|fr| super::render(fr, fr.area(), &app)).unwrap();
            t.backend().buffer().content().iter().map(|c| c.symbol()).collect()
        };
        let frames: Vec<String> = (0..super::FAN_ROT_FRAMES).map(draw_frame).collect();
        for (f, frame) in frames.iter().enumerate() {
            let stars = frame.matches("✳").count();
            assert!((12..=48).contains(&stars), "frame {f}: {stars} stars out of range");
        }
        for f in 0..frames.len() {
            let next = (f + 1) % frames.len();
            assert_ne!(frames[f], frames[next], "frames {f}/{next} identical — no rotation");
        }
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
    fn full_tier_needs_nine_rows_for_unclipped_star_fan() {
        let full = draw_header(80, 9);
        assert!(full.contains("✳"), "star fan missing at height 9");
        for h in [5, 6, 7, 8] {
            let short = draw_header(80, h);
            assert!(!short.contains("✳"), "height {h} must fall back to compact");
        }
    }

    #[test]
    fn star_fan_alternates_the_two_brand_tones() {
        let mut t = Terminal::new(TestBackend::new(80, 9)).unwrap();
        let app = App::demo();
        t.draw(|f| super::render(f, f.area(), &app)).unwrap();
        let buf = t.backend().buffer().clone();
        let colors: Vec<_> = buf
            .content()
            .iter()
            .filter(|c| c.symbol() == "✳")
            .map(|c| c.style().fg.unwrap())
            .collect();
        assert!(colors.contains(&crate::ui::theme::ACCENT), "accent arms missing");
        assert!(colors.contains(&crate::ui::theme::TEXT), "white arms missing");
        for c in colors {
            assert!(
                c == crate::ui::theme::ACCENT || c == crate::ui::theme::TEXT,
                "off-brand color {c:?}"
            );
        }
    }
}
