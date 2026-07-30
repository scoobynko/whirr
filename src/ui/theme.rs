use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, BorderType, Borders};

use crate::sampler::PressureLevel;

pub const ACCENT: Color = Color::Rgb(45, 225, 194);      // teal
pub const AMBER: Color = Color::Rgb(255, 180, 84);
pub const RED: Color = Color::Rgb(255, 92, 87);
pub const GREEN: Color = Color::Rgb(74, 214, 109);
pub const TEXT: Color = Color::Rgb(205, 214, 217);
pub const DIM: Color = Color::Rgb(90, 105, 110);         // borders, labels
pub const BG_CELL: Color = Color::Rgb(18, 32, 36);       // empty heatmap cell

const GRAD_FROM: (u8, u8, u8) = (14, 58, 58); // dark teal
const GRAD_TO: (u8, u8, u8) = (45, 225, 194);

pub fn gradient(t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t) as u8;
    Color::Rgb(
        lerp(GRAD_FROM.0, GRAD_TO.0),
        lerp(GRAD_FROM.1, GRAD_TO.1),
        lerp(GRAD_FROM.2, GRAD_TO.2),
    )
}

/// Linear RGB interpolation, used by the burst fan to dim partially covered
/// cells toward the background (its stand-in for anti-aliasing, since a cell
/// carries one foreground and no coverage information).
pub fn blend(from: Color, to: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    let ch = |c: Color| match c {
        Color::Rgb(r, g, b) => (r, g, b),
        other => panic!("blend expects Color::Rgb, got {other:?}"),
    };
    let (fr, fg, fb) = ch(from);
    let (tr, tg, tb) = ch(to);
    let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t) as u8;
    Color::Rgb(lerp(fr, tr), lerp(fg, tg), lerp(fb, tb))
}

pub fn temp_color(c: f32) -> Color {
    if c >= 95.0 { RED } else if c >= 85.0 { AMBER } else { ACCENT }
}

pub fn pressure_color(p: PressureLevel) -> Color {
    match p {
        PressureLevel::Normal => GREEN,
        PressureLevel::Warn => AMBER,
        PressureLevel::Critical => RED,
    }
}

pub fn panel_block(title: &str, focused: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused { ACCENT } else { DIM }))
        .title(format!(" {title} "))
        .title_style(Style::default().fg(TEXT))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    #[test]
    fn gradient_is_monotonic_and_clamped() {
        let ch = |c: Color| match c { Color::Rgb(r, g, b) => (r, g, b), _ => panic!() };
        let (r0, g0, b0) = ch(gradient(0.0));
        let (r1, g1, b1) = ch(gradient(1.0));
        assert_eq!((r1, g1, b1), (45, 225, 194));
        assert!(g0 < g1 && b0 < b1 && r0 <= r1);
        assert_eq!(gradient(-1.0), gradient(0.0));
        assert_eq!(gradient(2.0), gradient(1.0));
    }

    #[test]
    fn temp_thresholds() {
        assert_eq!(temp_color(60.0), ACCENT);
        assert_eq!(temp_color(85.0), AMBER);
        assert_eq!(temp_color(95.0), RED);
    }

    #[test]
    fn blend_hits_both_endpoints_and_the_midpoint() {
        let a = Color::Rgb(0, 0, 0);
        let b = Color::Rgb(100, 200, 50);
        assert_eq!(blend(a, b, 0.0), a);
        assert_eq!(blend(a, b, 1.0), b);
        assert_eq!(blend(a, b, 0.5), Color::Rgb(50, 100, 25));
    }

    #[test]
    fn blend_clamps_out_of_range_t() {
        let a = Color::Rgb(10, 20, 30);
        let b = Color::Rgb(200, 210, 220);
        assert_eq!(blend(a, b, -1.0), a);
        assert_eq!(blend(a, b, 5.0), b);
    }
}
