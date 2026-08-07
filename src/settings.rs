//! What the user has chosen, and the palette that follows from it.
//!
//! Kept apart from `ui::theme` because a `Theme` is the *result* — eleven
//! resolved colours — while these are the handful of choices a person
//! actually makes. `Settings::theme()` is the only bridge between them, which
//! keeps the widgets ignorant of how their colours were picked.

use std::path::PathBuf;

use ratatui::style::Color;
use serde::{Deserialize, Serialize};

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

/// The on-disk shape, kept separate from `Settings` so the file format can
/// stay stable while the in-memory type moves.
///
/// Every field is `Option<String>` rather than a typed enum: serde would
/// reject the *whole file* over one unrecognised value, so a typo in the
/// accent would silently discard a deliberate theme choice. Parsing each
/// field by hand keeps a bad value costing only itself.
#[derive(Serialize, Deserialize, Default)]
struct ConfigFile {
    #[serde(default)]
    appearance: Appearance,
    #[serde(default)]
    behaviour: Behaviour,
}

#[derive(Serialize, Deserialize, Default)]
struct Appearance {
    theme: Option<String>,
    accent: Option<String>,
    background: Option<String>,
}

#[derive(Serialize, Deserialize, Default)]
struct Behaviour {
    fan: Option<bool>,
}

impl Palette {
    fn from_label(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.label() == s)
    }
}

impl Accent {
    fn from_label(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.label() == s)
    }
}

impl Settings {
    /// Where the choices are remembered between runs.
    pub fn config_path() -> Option<PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
        Some(base.join("whirr").join("config.toml"))
    }

    /// Read the config, or fall back to defaults.
    ///
    /// Never fails: a dashboard that refuses to start over a stray character
    /// in a preferences file would be worse than one that ignores it.
    pub fn load() -> Self {
        Self::config_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|t| Self::from_toml(&t))
            .unwrap_or_default()
    }

    /// Write the current choices. Errors are dropped — failing to persist a
    /// preference is not worth interrupting the dashboard for.
    pub fn save(&self) {
        let Some(path) = Self::config_path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, self.to_toml());
    }

    /// Apply whatever the file got right, on top of the defaults.
    pub fn from_toml(text: &str) -> Self {
        let file: ConfigFile = toml::from_str(text).unwrap_or_default();
        let mut s = Self::default();
        if let Some(p) = file.appearance.theme.as_deref().and_then(Palette::from_label) {
            s.palette = p;
        }
        if let Some(a) = file.appearance.accent.as_deref().and_then(Accent::from_label) {
            s.accent = a;
        }
        match file.appearance.background.as_deref() {
            Some("terminal") => s.terminal_bg = true,
            Some("painted") => s.terminal_bg = false,
            _ => {}
        }
        if let Some(fan) = file.behaviour.fan {
            s.fan = fan;
        }
        s
    }

    pub fn to_toml(&self) -> String {
        let file = ConfigFile {
            appearance: Appearance {
                theme: Some(self.palette.label().to_string()),
                accent: Some(self.accent.label().to_string()),
                background: Some(
                    if self.terminal_bg { "terminal" } else { "painted" }.to_string(),
                ),
            },
            behaviour: Behaviour { fan: Some(self.fan) },
        };
        // The only way this fails is a bug in this function, not in user input.
        toml::to_string_pretty(&file).unwrap_or_default()
    }

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
    fn a_written_config_reads_back_as_the_same_settings() {
        let s = Settings {
            palette: Palette::Light,
            accent: Accent::Violet,
            terminal_bg: true,
            fan: false,
        };
        assert_eq!(Settings::from_toml(&s.to_toml()), s);
    }

    #[test]
    fn the_file_reads_the_way_a_person_would_write_it() {
        let text = Settings::default().to_toml();
        assert!(text.contains("[appearance]"), "{text}");
        assert!(text.contains("theme = \"dark\""), "{text}");
        assert!(text.contains("background = \"painted\""), "{text}");
        assert!(text.contains("[behaviour]"), "{text}");
    }

    #[test]
    fn a_partial_config_keeps_the_defaults_for_everything_it_omits() {
        // A file written by an older whirr must not reset the settings it
        // never knew about.
        let s = Settings::from_toml("[appearance]\ntheme = \"light\"\n");
        assert_eq!(s.palette, Palette::Light);
        assert_eq!(s.accent, Settings::default().accent, "omitted keys keep their default");
        assert_eq!(s.fan, Settings::default().fan);
    }

    #[test]
    fn one_bad_value_costs_only_that_setting() {
        // Per-field tolerance rather than all-or-nothing: a typo in the accent
        // must not silently throw away a deliberate theme choice.
        let s = Settings::from_toml(
            "[appearance]\ntheme = \"light\"\naccent = \"chartreuse\"\n",
        );
        assert_eq!(s.palette, Palette::Light, "the valid setting survives");
        assert_eq!(s.accent, Settings::default().accent, "the invalid one falls back");
    }

    #[test]
    fn an_unreadable_config_is_ignored_rather_than_fatal() {
        // A dashboard that refuses to start because of a stray character in a
        // preferences file would be worse than one that ignores it.
        for junk in ["", "not toml at all {{{", "[appearance", "theme = "] {
            assert_eq!(Settings::from_toml(junk), Settings::default(), "{junk:?}");
        }
    }

    #[test]
    fn unknown_keys_from_a_newer_whirr_are_ignored() {
        let s = Settings::from_toml(
            "[appearance]\ntheme = \"light\"\nsparkles = true\n\n[future]\nx = 1\n",
        );
        assert_eq!(s.palette, Palette::Light);
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
