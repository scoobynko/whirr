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

// Full-tier star fan: 8 arms of ✳ cells radiating from an empty hub,
// clockwise from N, each arm two (row, col) cells on a FAN_H x FAN_W grid.
// Odd columns keep diagonal/horizontal arms visually 45°/90° despite the
// ~2:1 cell aspect. Animation: one arm is always hidden and the gap
// advances one position per frame, so the wheel reads as rotating without
// any color change.
const FAN_H: usize = 5;
const FAN_W: usize = 11;
const FAN_ARMS: [[(usize, usize); 2]; 8] = [
    [(1, 5), (0, 5)], // N
    [(1, 7), (0, 9)], // NE
    [(2, 7), (2, 9)], // E
    [(3, 7), (4, 9)], // SE
    [(3, 5), (4, 5)], // S
    [(3, 3), (4, 1)], // SW
    [(2, 3), (2, 1)], // W
    [(1, 3), (0, 1)], // NW
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
    let mut grid = [[false; FAN_W]; FAN_H];
    let gap = frame % FAN_ARMS.len();
    for (i, arm) in FAN_ARMS.iter().enumerate() {
        if i == gap {
            continue;
        }
        for &(r, c) in arm {
            grid[r][c] = true;
        }
    }
    let style = Style::default().fg(theme::ACCENT);
    let lines: Vec<Line> = grid
        .iter()
        .map(|row| {
            Line::from(
                row.iter()
                    .map(|&on| if on { Span::styled("✳", style) } else { Span::raw(" ") })
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
    fn fan_arms_are_eight_distinct_two_cell_arms_in_bounds() {
        assert_eq!(super::FAN_ARMS.len(), 8);
        let mut seen = std::collections::HashSet::new();
        for arm in super::FAN_ARMS {
            for (r, c) in arm {
                assert!(r < super::FAN_H && c < super::FAN_W, "cell ({r},{c}) out of bounds");
                assert!(seen.insert((r, c)), "cell ({r},{c}) used by two arms");
            }
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
    fn full_tier_needs_seven_rows_for_unclipped_star_fan() {
        let full = draw_header(80, 7);
        assert_eq!(full.matches("✳").count(), 14, "7 of 8 arms x 2 star cells at height 7");
        for h in [5, 6] {
            let short = draw_header(80, h);
            assert!(!short.contains("✳"), "height {h} must fall back to compact");
        }
    }

    #[test]
    fn star_fan_gap_rotates_between_frames_in_brand_color() {
        let stars = |frame: usize| -> Vec<usize> {
            let mut t = Terminal::new(TestBackend::new(80, 7)).unwrap();
            let mut app = App::demo();
            app.fan_frame = frame;
            t.draw(|f| super::render(f, f.area(), &app)).unwrap();
            let buf = t.backend().buffer().clone();
            buf.content()
                .iter()
                .enumerate()
                .filter(|(_, c)| c.symbol() == "✳")
                .inspect(|(_, c)| {
                    assert_eq!(
                        c.style().fg,
                        Some(crate::ui::theme::ACCENT),
                        "star cells must use the brand accent color"
                    );
                })
                .map(|(i, _)| i)
                .collect()
        };
        let (f0, f1) = (stars(0), stars(1));
        assert_eq!(f0.len(), 14, "one arm must be hidden as the rotating gap");
        assert_ne!(f0, f1, "the gap must move between consecutive frames");
    }
}
