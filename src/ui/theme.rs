//! The palette, as a value rather than a set of constants.
//!
//! These used to be `pub const`s read directly by every widget, which meant
//! the palette was fixed at compile time and nothing could offer the user a
//! choice. `Theme` is `Copy` and lives on `App`, so a widget that already has
//! `&App` needs no new argument; the handful of helpers that take no `App`
//! (`gauge::frame`, `spark::render`, `burst::render`) take a `&Theme`.
//!
//! Deliberately *not* a global: rendering is single-threaded today, but a
//! process-wide mutable palette is easy to introduce and hard to remove, and
//! the explicit version turned out to cost only seventeen call sites.

use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, BorderType, Borders};

use crate::sampler::PressureLevel;

/// Every colour the UI draws with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Theme {
    pub accent: Color,
    pub amber: Color,
    pub red: Color,
    pub green: Color,
    pub text: Color,
    /// Borders and labels.
    pub dim: Color,
    /// Empty heatmap cell. Also used as a *foreground* for text-on-accent
    /// (e.g. selected rows) — do not repurpose it as a background.
    pub bg_cell: Color,
    /// The frame background.
    pub base: Color,
    /// Dialog surface: reads as raised above `base` without competing with
    /// `accent`.
    pub bg_modal: Color,
    /// Ends of the heatmap/gauge gradient.
    pub grad_from: (u8, u8, u8),
    pub grad_to: (u8, u8, u8),
    /// Whether `draw` fills the frame with `base`.
    ///
    /// Separate from `base` on purpose. "Use the terminal's own background"
    /// cannot be expressed as `base: Color::Reset`, because `base` is also the
    /// colour `ramp` and `blend` darken toward — and `blend` panics on
    /// anything that isn't `Rgb`, on every frame of the burst fan. So the
    /// colour stays real and only the fill is skipped.
    pub paint_bg: bool,
}

impl Default for Theme {
    fn default() -> Self {
        Self::dark()
    }
}

impl Theme {
    /// The palette whirr has always shipped: near-black frame with a slight
    /// cool cast, teal accent.
    pub const fn dark() -> Self {
        Self {
            accent: Color::Rgb(45, 225, 194),
            amber: Color::Rgb(255, 180, 84),
            red: Color::Rgb(255, 92, 87),
            green: Color::Rgb(74, 214, 109),
            text: Color::Rgb(205, 214, 217),
            dim: Color::Rgb(90, 105, 110),
            bg_cell: Color::Rgb(18, 32, 36),
            base: Color::Rgb(10, 14, 18),
            bg_modal: Color::Rgb(22, 30, 36),
            grad_from: (14, 58, 58),
            grad_to: (45, 225, 194),
            paint_bg: true,
        }
    }

    /// A hand-picked light palette, not an inversion of the dark one.
    ///
    /// Inverting would keep every colour's contrast tuned for near-black:
    /// the teal accent and the dim grey both lose most of their separation
    /// against a near-white field and read as washed out. These are chosen
    /// against this background instead — darker, more saturated, and with the
    /// greys re-picked so borders stay quiet without disappearing.
    pub const fn light() -> Self {
        Self {
            accent: Color::Rgb(0, 132, 112),
            amber: Color::Rgb(160, 90, 0),
            red: Color::Rgb(190, 40, 38),
            green: Color::Rgb(28, 122, 54),
            text: Color::Rgb(28, 38, 44),
            dim: Color::Rgb(118, 132, 138),
            bg_cell: Color::Rgb(224, 231, 233),
            base: Color::Rgb(246, 248, 249),
            bg_modal: Color::Rgb(255, 255, 255),
            grad_from: (200, 228, 222),
            grad_to: (0, 132, 112),
            paint_bg: true,
        }
    }

    pub fn gradient(&self, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        let lerp = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t) as u8;
        Color::Rgb(
            lerp(self.grad_from.0, self.grad_to.0),
            lerp(self.grad_from.1, self.grad_to.1),
            lerp(self.grad_from.2, self.grad_to.2),
        )
    }

    /// Darken `color` toward `base` by factor `t` (0.0 = `base`, 1.0 = the
    /// full `color`). Used by sparkline bars so a short bar is a dim version
    /// of the chart's own colour and a tall bar is the full colour, instead of
    /// every bar being one flat shade. Any `t` above zero is floored to
    /// `MIN_RAMP` so a bar that exists at all stays visible; `t == 0.0` (no
    /// bar) still renders as exactly `base`.
    pub fn ramp(&self, color: Color, t: f32) -> Color {
        let t = t.clamp(0.0, 1.0);
        let floored = if t > 0.0 { t.max(MIN_RAMP) } else { 0.0 };
        blend(self.base, color, floored)
    }

    pub fn temp_color(&self, c: f32) -> Color {
        if c >= 95.0 {
            self.red
        } else if c >= 85.0 {
            self.amber
        } else {
            self.accent
        }
    }

    pub fn pressure_color(&self, p: PressureLevel) -> Color {
        match p {
            PressureLevel::Normal => self.green,
            PressureLevel::Warn => self.amber,
            PressureLevel::Critical => self.red,
        }
    }

    pub fn panel_block(&self, title: &str, focused: bool) -> Block<'static> {
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(if focused { self.accent } else { self.dim }))
            .title(format!(" {title} "))
            .title_style(Style::default().fg(self.text))
    }
}

/// Below this ramp factor a non-zero value is floored up to it, so the
/// dimmest visible bar still reads as distinct from `base` instead of
/// blending almost exactly into it. Matches `burst::MIN_BRIGHT` — the same
/// problem (a colour blended toward the background disappearing at low
/// coverage) gets the same fix.
const MIN_RAMP: f32 = 0.5;

/// Linear RGB interpolation, used by the burst fan to dim partially covered
/// cells toward the background (its stand-in for anti-aliasing, since a cell
/// carries one foreground and no coverage information).
///
/// Free-standing rather than a method: both endpoints are given explicitly,
/// so it needs nothing from the palette.
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    /// These all describe the palette's *behaviour* (monotonic gradients,
    /// clamping, the ramp floor), which is independent of the values, so they
    /// run against the shipped dark palette.
    const TH: Theme = Theme::dark();

    fn ch(c: Color) -> (u8, u8, u8) {
        match c {
            Color::Rgb(r, g, b) => (r, g, b),
            other => panic!("expected Rgb, got {other:?}"),
        }
    }

    #[test]
    fn gradient_is_monotonic_and_clamped() {
        let (r0, g0, b0) = ch(TH.gradient(0.0));
        let (r1, g1, b1) = ch(TH.gradient(1.0));
        assert_eq!((r1, g1, b1), (45, 225, 194));
        assert!(g0 < g1 && b0 < b1 && r0 <= r1);
        assert_eq!(TH.gradient(-1.0), TH.gradient(0.0));
        assert_eq!(TH.gradient(2.0), TH.gradient(1.0));
    }

    #[test]
    fn temp_thresholds() {
        assert_eq!(TH.temp_color(60.0), TH.accent);
        assert_eq!(TH.temp_color(85.0), TH.amber);
        assert_eq!(TH.temp_color(95.0), TH.red);
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

    #[test]
    fn ramp_hits_base_at_zero_and_the_full_colour_at_one() {
        assert_eq!(TH.ramp(TH.accent, 0.0), TH.base);
        assert_eq!(TH.ramp(TH.accent, 1.0), TH.accent);
    }

    #[test]
    fn ramp_is_dimmer_at_low_factors_than_high_ones() {
        let (r_dim, g_dim, b_dim) = ch(TH.ramp(TH.accent, 0.2));
        let (r_bright, g_bright, b_bright) = ch(TH.ramp(TH.accent, 1.0));
        assert!(r_dim <= r_bright && g_dim < g_bright && b_dim < b_bright);
    }

    #[test]
    fn ramp_floors_the_lowest_nonzero_bar_away_from_base() {
        // A bar at a sliver of its max (well below MIN_RAMP) must still
        // render meaningfully distinct from base, not fade into it — the
        // same defect burst.rs already fixed for its fringe dots.
        let dimmest = TH.ramp(TH.accent, 0.01);
        assert_ne!(dimmest, TH.base);
        let d = |a: u8, b: u8| i32::from(a).abs_diff(i32::from(b)) as i32;
        let (r0, g0, b0) = ch(TH.base);
        let (r1, g1, b1) = ch(dimmest);
        let dist = d(r1, r0) + d(g1, g0) + d(b1, b0);
        assert!(dist > 20, "lowest non-zero bar colour {dimmest:?} too close to base");
        // A genuinely absent bar (t == 0.0) must still render as exactly base.
        assert_eq!(TH.ramp(TH.accent, 0.0), TH.base);
    }

    #[test]
    fn ramp_stays_within_a_colours_own_hue_not_teal() {
        // A second hue ramped toward base should never pass through accent's
        // own channel proportions — proves the ramp derives from the caller's
        // colour, not a hardcoded teal gradient.
        assert_ne!(TH.ramp(TH.red, 0.5), TH.ramp(TH.accent, 0.5));
    }

    #[test]
    fn a_theme_is_copied_not_shared() {
        // The whole reason this is a value rather than a global: two Apps can
        // hold different palettes without one seeing the other's.
        let mut other = Theme::dark();
        other.accent = Color::Rgb(1, 2, 3);
        assert_eq!(TH.accent, Theme::dark().accent, "mutating a copy must not touch the original");
        assert_ne!(other.accent, TH.accent);
    }
}
