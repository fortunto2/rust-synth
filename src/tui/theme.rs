//! Blade-Runner-style visual theme.
//!
//! Every widget pulls its colours from a `Theme` struct so the entire
//! mix recolours atomically when the user switches scene or vibe. The
//! default palette is the Night City scheme — warm-black background,
//! amber-CRT primary, neon-cyan for active voices, blood-red reserved
//! for clipping and modal errors, bone-white for dim secondary text.

use ratatui::style::Color;

/// Named five-colour palette. No pure white, no pure green — those
/// don't exist in Blade Runner.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg_deep: Color,   // warm black
    pub amber: Color,     // primary
    pub cyan: Color,      // active / selected
    pub red: Color,       // clip / warning
    pub bone: Color,      // dim secondary
    pub dust: Color,      // dim amber (inactive)
}

impl Palette {
    pub const NIGHT_CITY: Palette = Palette {
        bg_deep: Color::Rgb(10, 9, 7),
        amber: Color::Rgb(255, 165, 82),
        cyan: Color::Rgb(77, 208, 225),
        red: Color::Rgb(193, 39, 45),
        bone: Color::Rgb(232, 219, 197),
        dust: Color::Rgb(184, 92, 42),
    };
}

/// Thematic glyphs. Separated from the palette so a monochrome port
/// could re-use the ASCII language without colour.
pub struct Glyphs;
impl Glyphs {
    // Frame primary (straight right angles, Wallace-Corp aesthetic).
    pub const TL: &'static str = "┌";
    pub const TR: &'static str = "┐";
    pub const BL: &'static str = "└";
    pub const BR: &'static str = "┘";
    pub const H: &'static str = "─";
    pub const V: &'static str = "│";
    pub const CROSS: &'static str = "┼";

    // Dividers.
    pub const HEAVY: &'static str = "━";
    pub const THIN_DOTS: &'static str = "· ";

    // VU / levels.
    pub const LEVELS: [&'static str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

    // Voice LEDs.
    pub const LED_ON: &'static str = "◉";
    pub const LED_OFF: &'static str = "○";
    pub const LED_DIM: &'static str = "◌";
    pub const LED_SEL: &'static str = "◎";

    // Corner decorations (used around panels that deserve emphasis).
    pub const CORNER_TL: &'static str = "◢";
    pub const CORNER_TR: &'static str = "◣";

    // Value brackets.
    pub const BRACKET_OPEN: &'static str = "⟨";
    pub const BRACKET_CLOSE: &'static str = "⟩";

    // Kanji texture (dim-amber, treated as background noise).
    pub const KANJI_TEXTURE: &'static str = "非常口 · 警告 · 株式会社 · 企業";
}

/// Active theme — chosen by the user. `current()` is what every widget
/// calls to colour itself. Replacing this struct is a single atomic
/// colour change across the whole UI.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub palette: Palette,
    pub vu_pulse_hz: f64,
}

impl Theme {
    pub const NIGHT_CITY: Theme = Theme {
        palette: Palette::NIGHT_CITY,
        vu_pulse_hz: 1.2,
    };

    /// The single theme the app currently uses. One call-site to
    /// change when we ship alternate palettes — widgets don't
    /// reference a specific const.
    #[inline]
    pub fn current() -> Theme {
        Theme::NIGHT_CITY
    }
}

/// Convenience colour helpers used by many widgets.
impl Theme {
    pub fn fg(self) -> Color {
        self.palette.amber
    }
    pub fn fg_dim(self) -> Color {
        self.palette.dust
    }
    pub fn accent(self) -> Color {
        self.palette.cyan
    }
    pub fn warn(self) -> Color {
        self.palette.red
    }
    pub fn secondary(self) -> Color {
        self.palette.bone
    }
    pub fn bg(self) -> Color {
        self.palette.bg_deep
    }
}
