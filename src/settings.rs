//! What the user has chosen, and the palette that follows from it.
//!
//! Kept apart from `ui::theme` because a `Theme` is the *result* — eleven
//! resolved colours — while these are the handful of choices a person
//! actually makes. `Settings::theme()` is the only bridge between them, which
//! keeps the widgets ignorant of how their colours were picked.

use ratatui::style::Color;

use crate::ui::theme::Theme;

/// Which base palette to build on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Palette {
    Dark,
    Light,
}

impl Palette {
    pub const ALL: [Palette; 2] = [Palette::Dark, Palette::Light];

    pub fn next(self) -> Self {
        match self {
            Palette::Dark => Palette::Light,
            Palette::Light => Palette::Dark,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Palette::Dark => "dark",
            Palette::Light => "light",
        }
    }
}

/// The one colour that is genuinely a matter of taste. It drives gauges,
/// selection, sparklines and the burst fan, so it gets its own choice rather
/// than being welded to the palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Accent {
    Teal,
    Blue,
    Violet,
    Amber,
    Green,
}

impl Accent {
    pub const ALL: [Accent; 5] =
        [Accent::Teal, Accent::Blue, Accent::Violet, Accent::Amber, Accent::Green];

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|&a| a == self).expect("ALL covers every Accent");
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    pub fn label(self) -> &'static str {
        match self {
            Accent::Teal => "teal",
            Accent::Blue => "blue",
            Accent::Violet => "violet",
            Accent::Amber => "amber",
            Accent::Green => "green",
        }
    }

    /// Two versions of each: a light background needs a darker, more saturated
    /// accent to hold the same contrast a bright one holds against near-black.
    fn color(self, palette: Palette) -> Color {
        match (self, palette) {
            (Accent::Teal, Palette::Dark) => Color::Rgb(45, 225, 194),
            (Accent::Teal, Palette::Light) => Color::Rgb(0, 132, 112),
            (Accent::Blue, Palette::Dark) => Color::Rgb(94, 178, 255),
            (Accent::Blue, Palette::Light) => Color::Rgb(21, 101, 192),
            (Accent::Violet, Palette::Dark) => Color::Rgb(178, 148, 255),
            (Accent::Violet, Palette::Light) => Color::Rgb(94, 53, 177),
            (Accent::Amber, Palette::Dark) => Color::Rgb(255, 180, 84),
            (Accent::Amber, Palette::Light) => Color::Rgb(160, 90, 0),
            (Accent::Green, Palette::Dark) => Color::Rgb(74, 214, 109),
            (Accent::Green, Palette::Light) => Color::Rgb(28, 122, 54),
        }
    }

    /// The dim end of the heatmap gradient.
    ///
    /// A table rather than "the accent mixed 25% into the background", for the
    /// same reason the light palette is hand-picked: the shipped teal-on-dark
    /// gradient starts at (14, 58, 58), and no single mix ratio reproduces it
    /// — it was tuned by eye. Computing it would have quietly shifted the
    /// default look, which is the one thing this feature must not do.
    fn gradient_from(self, palette: Palette) -> (u8, u8, u8) {
        match (self, palette) {
            (Accent::Teal, Palette::Dark) => (14, 58, 58),
            (Accent::Teal, Palette::Light) => (200, 228, 222),
            (Accent::Blue, Palette::Dark) => (16, 48, 76),
            (Accent::Blue, Palette::Light) => (190, 211, 235),
            (Accent::Violet, Palette::Dark) => (46, 42, 78),
            (Accent::Violet, Palette::Light) => (208, 199, 231),
            (Accent::Amber, Palette::Dark) => (68, 50, 28),
            (Accent::Amber, Palette::Light) => (225, 209, 187),
            (Accent::Green, Palette::Dark) => (20, 60, 34),
            (Accent::Green, Palette::Light) => (192, 217, 200),
        }
    }
}

/// Everything the settings dialog can change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub palette: Palette,
    pub accent: Accent,
    /// Leave the frame unpainted so the terminal's own background — including
    /// transparency and blur — shows through.
    pub terminal_bg: bool,
    pub fan: bool,
}

impl Default for Settings {
    /// Exactly what whirr shipped before any of this existed. A default that
    /// changes the look would make the feature a redesign.
    fn default() -> Self {
        Self { palette: Palette::Dark, accent: Accent::Teal, terminal_bg: false, fan: true }
    }
}

impl Settings {
    /// Resolve the choices into the eleven colours the widgets draw with.
    pub fn theme(&self) -> Theme {
        let base = match self.palette {
            Palette::Dark => Theme::dark(),
            Palette::Light => Theme::light(),
        };
        let accent = self.accent.color(self.palette);
        Theme {
            accent,
            grad_from: self.accent.gradient_from(self.palette),
            grad_to: match accent {
                Color::Rgb(r, g, b) => (r, g, b),
                _ => base.grad_to,
            },
            // `base` keeps its colour even when the frame is left unpainted:
            // it is what `ramp` and `blend` darken toward, and `Color::Reset`
            // there would panic inside `blend` on every frame of the fan.
            paint_bg: !self.terminal_bg,
            ..base
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Color;

    fn rgb(c: Color) -> (u8, u8, u8) {
        match c {
            Color::Rgb(r, g, b) => (r, g, b),
            other => panic!("the palette must be Rgb throughout, got {other:?}"),
        }
    }

    fn luma(c: Color) -> f32 {
        let (r, g, b) = rgb(c);
        0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32
    }

    #[test]
    fn the_light_palette_is_actually_light_and_the_dark_one_dark() {
        let dark = Settings { palette: Palette::Dark, ..Settings::default() }.theme();
        let light = Settings { palette: Palette::Light, ..Settings::default() }.theme();
        assert!(luma(dark.base) < 40.0, "dark base should be near-black");
        assert!(luma(light.base) > 200.0, "light base should be near-white");
        // Text has to invert with it, or a light theme is unreadable.
        assert!(luma(dark.text) > luma(dark.base), "dark theme needs light text");
        assert!(luma(light.text) < luma(light.base), "light theme needs dark text");
    }

    #[test]
    fn a_light_palette_is_not_an_inversion_of_the_dark_one() {
        // Every colour hand-picked for its background. If light were just
        // dark with base and text swapped, the accent and the dim would keep
        // dark-background contrast ratios and read as washed out.
        let dark = Settings { palette: Palette::Dark, ..Settings::default() }.theme();
        let light = Settings { palette: Palette::Light, ..Settings::default() }.theme();
        for (name, d, l) in [
            ("accent", dark.accent, light.accent),
            ("dim", dark.dim, light.dim),
            ("bg_cell", dark.bg_cell, light.bg_cell),
        ] {
            assert_ne!(d, l, "{name} must be picked for its own background");
        }
    }

    #[test]
    fn every_palette_and_accent_stays_rgb() {
        // `theme::blend` panics on anything but Rgb, and it runs on every
        // frame of the burst fan. A palette carrying Color::Reset would take
        // the whole dashboard down.
        for palette in Palette::ALL {
            for accent in Accent::ALL {
                let t = Settings { palette, accent, ..Settings::default() }.theme();
                for c in [t.accent, t.amber, t.red, t.green, t.text, t.dim, t.bg_cell, t.base, t.bg_modal] {
                    rgb(c);
                }
            }
        }
    }

    #[test]
    fn choosing_an_accent_overrides_the_palettes_own() {
        let teal = Settings { accent: Accent::Teal, ..Settings::default() }.theme();
        let violet = Settings { accent: Accent::Violet, ..Settings::default() }.theme();
        assert_ne!(teal.accent, violet.accent);
        // The gradient is built from the accent, so it has to follow.
        assert_ne!(teal.gradient(1.0), violet.gradient(1.0));
        assert_eq!(violet.gradient(1.0), violet.accent, "gradient should top out at the accent");
    }

    #[test]
    fn using_the_terminal_background_leaves_the_blend_colour_intact() {
        // The frame simply isn't painted; `base` keeps its real colour so
        // `ramp` and `blend` still have something to darken toward. Setting it
        // to Color::Reset instead would panic in blend.
        let painted = Settings { terminal_bg: false, ..Settings::default() }.theme();
        let bare = Settings { terminal_bg: true, ..Settings::default() }.theme();
        assert_eq!(painted.base, bare.base, "base is the blend anchor, not just a fill");
        assert!(painted.paint_bg && !bare.paint_bg);
        rgb(bare.ramp(bare.accent, 0.5));
    }

    #[test]
    fn settings_start_as_the_palette_whirr_has_always_shipped() {
        let d = Settings::default();
        assert_eq!(d.theme(), crate::ui::theme::Theme::dark(), "defaults must not change the look");
        assert!(d.fan, "the fan is on unless asked otherwise");
    }

    #[test]
    fn each_choice_cycles_through_all_of_its_options_and_wraps() {
        let mut p = Palette::Dark;
        for _ in 0..Palette::ALL.len() {
            p = p.next();
        }
        assert_eq!(p, Palette::Dark, "cycling every option returns to the start");

        let mut a = Accent::Teal;
        let mut seen = vec![a];
        for _ in 1..Accent::ALL.len() {
            a = a.next();
            assert!(!seen.contains(&a), "cycle must not repeat before it wraps");
            seen.push(a);
        }
        assert_eq!(a.next(), Accent::Teal);
    }
}
