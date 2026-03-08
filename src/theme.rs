//! Color theme types with custom serde support.
//!
//! [`ThemeColor`] is a newtype wrapping [`ratatui::style::Color`] that supports
//! human-readable color strings in TOML config:
//! - Named colors: `"black"`, `"white"`, `"red"`, `"green"`, `"blue"`, etc.
//! - Indexed terminal colors: `"color0"` through `"color255"`
//! - Hex RGB: `"#rrggbb"`
//! - Special: `"reset"`, `"transparent"`
//!
//! [`ColorScheme`] groups a foreground and background [`ThemeColor`] for a UI component.

use ratatui::style::Color;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;

// ─── ThemeColor ──────────────────────────────────────────────────────────────

/// A `ratatui::style::Color` newtype with custom serde (human-readable strings).
///
/// Supported TOML formats:
/// - Named: `"black"`, `"white"`, `"red"`, `"green"`, `"blue"`, `"yellow"`,
///   `"cyan"`, `"magenta"`, `"gray"`, `"darkgray"`, `"lightred"`, `"lightgreen"`,
///   `"lightyellow"`, `"lightblue"`, `"lightmagenta"`, `"lightcyan"`, `"transparent"`
/// - Indexed: `"color0"` through `"color255"`
/// - Hex: `"#rrggbb"`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeColor(pub Color);

impl ThemeColor {
    pub fn into_color(self) -> Color {
        self.0
    }
}

impl Default for ThemeColor {
    fn default() -> Self {
        ThemeColor(Color::Reset)
    }
}

impl From<Color> for ThemeColor {
    fn from(c: Color) -> Self {
        ThemeColor(c)
    }
}

impl From<ThemeColor> for Color {
    fn from(tc: ThemeColor) -> Self {
        tc.0
    }
}

impl fmt::Display for ThemeColor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Color::Reset => write!(f, "transparent"),
            Color::Black => write!(f, "black"),
            Color::Red => write!(f, "red"),
            Color::Green => write!(f, "green"),
            Color::Yellow => write!(f, "yellow"),
            Color::Blue => write!(f, "blue"),
            Color::Magenta => write!(f, "magenta"),
            Color::Cyan => write!(f, "cyan"),
            Color::Gray => write!(f, "gray"),
            Color::DarkGray => write!(f, "darkgray"),
            Color::LightRed => write!(f, "lightred"),
            Color::LightGreen => write!(f, "lightgreen"),
            Color::LightYellow => write!(f, "lightyellow"),
            Color::LightBlue => write!(f, "lightblue"),
            Color::LightMagenta => write!(f, "lightmagenta"),
            Color::LightCyan => write!(f, "lightcyan"),
            Color::White => write!(f, "white"),
            Color::Indexed(i) => write!(f, "color{i}"),
            Color::Rgb(r, g, b) => write!(f, "#{r:02x}{g:02x}{b:02x}"),
        }
    }
}

impl FromStr for ThemeColor {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lower = s.to_lowercase();
        let color = match lower.as_str() {
            "black" => Color::Black,
            "red" => Color::Red,
            "green" => Color::Green,
            "yellow" => Color::Yellow,
            "blue" => Color::Blue,
            "magenta" => Color::Magenta,
            "cyan" => Color::Cyan,
            "gray" | "grey" => Color::Gray,
            "darkgray" | "darkgrey" | "dark_gray" | "dark_grey" => Color::DarkGray,
            "lightred" | "light_red" => Color::LightRed,
            "lightgreen" | "light_green" => Color::LightGreen,
            "lightyellow" | "light_yellow" => Color::LightYellow,
            "lightblue" | "light_blue" => Color::LightBlue,
            "lightmagenta" | "light_magenta" => Color::LightMagenta,
            "lightcyan" | "light_cyan" => Color::LightCyan,
            "white" => Color::White,
            "reset" | "transparent" => Color::Reset,
            _ if lower.starts_with("color") => {
                let idx_str = &lower["color".len()..];
                let idx: u8 = idx_str.parse().map_err(|_| {
                    format!("invalid indexed color (must be color0–color255): {s:?}")
                })?;
                Color::Indexed(idx)
            }
            _ if s.starts_with('#') => parse_hex(s)?,
            _ => {
                return Err(format!(
                    "unknown color {s:?}; expected a named color, colorN, or #rrggbb"
                ))
            }
        };
        Ok(ThemeColor(color))
    }
}

fn parse_hex(s: &str) -> Result<Color, String> {
    let hex = s.trim_start_matches('#');
    if hex.len() != 6 {
        return Err(format!(
            "hex color must be exactly 6 hex digits, got: {s:?}"
        ));
    }
    let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| format!("invalid hex color: {s:?}"))?;
    let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| format!("invalid hex color: {s:?}"))?;
    let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| format!("invalid hex color: {s:?}"))?;
    Ok(Color::Rgb(r, g, b))
}

impl Serialize for ThemeColor {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ThemeColor {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

// ─── ColorScheme ─────────────────────────────────────────────────────────────

/// A foreground/background color pair for a UI component.
///
/// Used as a building block for simple two-color theming of individual
/// widgets or sections.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ColorScheme {
    pub fg: ThemeColor,
    pub bg: ThemeColor,
}

impl Default for ColorScheme {
    fn default() -> Self {
        Self {
            fg: ThemeColor(Color::White),
            bg: ThemeColor(Color::Reset),
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_named_colors() {
        assert_eq!(
            "black".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::Black)
        );
        assert_eq!(
            "white".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::White)
        );
        assert_eq!("red".parse::<ThemeColor>().unwrap(), ThemeColor(Color::Red));
        assert_eq!(
            "green".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::Green)
        );
        assert_eq!(
            "yellow".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::Yellow)
        );
        assert_eq!(
            "blue".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::Blue)
        );
        assert_eq!(
            "cyan".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::Cyan)
        );
        assert_eq!(
            "magenta".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::Magenta)
        );
        assert_eq!(
            "gray".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::Gray)
        );
        assert_eq!(
            "grey".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::Gray)
        );
        assert_eq!(
            "darkgray".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::DarkGray)
        );
        assert_eq!(
            "lightred".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::LightRed)
        );
        assert_eq!(
            "lightblue".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::LightBlue)
        );
        assert_eq!(
            "lightgreen".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::LightGreen)
        );
        assert_eq!(
            "lightyellow".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::LightYellow)
        );
        assert_eq!(
            "lightmagenta".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::LightMagenta)
        );
        assert_eq!(
            "lightcyan".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::LightCyan)
        );
    }

    #[test]
    fn parse_case_insensitive() {
        assert_eq!(
            "BLACK".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::Black)
        );
        assert_eq!(
            "LightRed".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::LightRed)
        );
        assert_eq!(
            "CYAN".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::Cyan)
        );
    }

    #[test]
    fn parse_special_colors() {
        assert_eq!(
            "reset".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::Reset)
        );
        assert_eq!(
            "transparent".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::Reset)
        );
        assert_eq!(
            "TRANSPARENT".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::Reset)
        );
    }

    #[test]
    fn parse_indexed_colors() {
        assert_eq!(
            "color0".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::Indexed(0))
        );
        assert_eq!(
            "color42".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::Indexed(42))
        );
        assert_eq!(
            "color128".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::Indexed(128))
        );
        assert_eq!(
            "color255".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::Indexed(255))
        );
        assert_eq!(
            "COLOR42".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::Indexed(42))
        );
    }

    #[test]
    fn parse_indexed_out_of_range_fails() {
        assert!("color256".parse::<ThemeColor>().is_err());
        assert!("color999".parse::<ThemeColor>().is_err());
    }

    #[test]
    fn parse_hex_colors() {
        assert_eq!(
            "#e06c75".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::Rgb(0xe0, 0x6c, 0x75))
        );
        assert_eq!(
            "#3E4452".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::Rgb(0x3e, 0x44, 0x52))
        );
        assert_eq!(
            "#000000".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::Rgb(0, 0, 0))
        );
        assert_eq!(
            "#ffffff".parse::<ThemeColor>().unwrap(),
            ThemeColor(Color::Rgb(255, 255, 255))
        );
    }

    #[test]
    fn parse_hex_invalid_fails() {
        assert!("#12345".parse::<ThemeColor>().is_err()); // too short
        assert!("#1234567".parse::<ThemeColor>().is_err()); // too long
        assert!("#gggggg".parse::<ThemeColor>().is_err()); // invalid hex digits
    }

    #[test]
    fn parse_unknown_fails() {
        assert!("notacolor".parse::<ThemeColor>().is_err());
        assert!("".parse::<ThemeColor>().is_err());
    }

    #[test]
    fn display_roundtrip() {
        let cases = [
            ThemeColor(Color::Red),
            ThemeColor(Color::White),
            ThemeColor(Color::Reset),
            ThemeColor(Color::Rgb(0xab, 0xcd, 0xef)),
            ThemeColor(Color::Indexed(42)),
            ThemeColor(Color::Indexed(0)),
            ThemeColor(Color::Indexed(255)),
        ];
        for tc in &cases {
            let s = tc.to_string();
            let parsed: ThemeColor = s
                .parse()
                .unwrap_or_else(|e| panic!("roundtrip failed for {s:?}: {e}"));
            assert_eq!(*tc, parsed, "roundtrip mismatch for {s:?}");
        }
    }

    #[test]
    fn serde_toml_roundtrip() {
        #[derive(Debug, Deserialize, Serialize)]
        struct Wrapper {
            color: ThemeColor,
        }

        let cases = ["red", "#e06c75", "color42", "transparent"];
        for &s in &cases {
            let toml = format!("color = \"{s}\"\n");
            let w: Wrapper =
                toml::from_str(&toml).unwrap_or_else(|e| panic!("failed to parse {s:?}: {e}"));
            let out = toml::to_string(&w).unwrap();
            let w2: Wrapper = toml::from_str(&out).unwrap();
            assert_eq!(w.color, w2.color, "serde roundtrip failed for {s:?}");
        }
    }

    #[test]
    fn color_scheme_default() {
        let cs = ColorScheme::default();
        assert_eq!(cs.fg, ThemeColor(Color::White));
        assert_eq!(cs.bg, ThemeColor(Color::Reset));
    }

    #[test]
    fn default_theme_color_is_reset() {
        assert_eq!(ThemeColor::default(), ThemeColor(Color::Reset));
    }
}
