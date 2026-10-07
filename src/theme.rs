//! Colour themes. The default is the Thermal Grizzly look (orange on black);
//! the others keep the same layout and only change the palette. Every theme
//! keeps its three status colours distinct from each other and from the
//! accent, and the text always says what a colour means.

use std::fmt;
use std::str::FromStr;

use crate::canvas::Rgb;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub name: &'static str,
    /// Background.
    pub surface: Rgb,
    /// Primary, secondary and tertiary text.
    pub ink: Rgb,
    pub ink2: Rgb,
    pub ink3: Rgb,
    /// Empty part of a bar.
    pub track: Rgb,
    /// Filled part of a bar within limits.
    pub accent: Rgb,
    pub warn: Rgb,
    pub crit: Rgb,
}

/// Thermal Grizzly orange on black (the original look).
pub const GRIZZLY: Theme = Theme {
    name: "grizzly",
    surface: [0, 0, 0],
    ink: [255, 255, 255],
    ink2: [185, 184, 176],
    ink3: [122, 121, 115],
    track: [38, 38, 36],
    accent: [240, 142, 51],
    warn: [250, 178, 25],
    crit: [208, 59, 59],
};

/// Yellow accent on black, in the spirit of iCUE; warnings turn orange.
pub const CORSAIR: Theme = Theme {
    name: "corsair",
    accent: [242, 196, 0],
    warn: [240, 142, 51],
    ..GRIZZLY
};

/// Cyan accent on black.
pub const ICE: Theme = Theme {
    name: "ice",
    accent: [56, 189, 248],
    ..GRIZZLY
};

/// Greyscale except for the two alarm colours.
pub const MONO: Theme = Theme {
    name: "mono",
    track: [42, 42, 42],
    accent: [230, 230, 230],
    ..GRIZZLY
};

/// The Nord palette: frost blue on polar night.
pub const NORD: Theme = Theme {
    name: "nord",
    surface: [46, 52, 64],
    ink: [236, 239, 244],
    ink2: [216, 222, 233],
    ink3: [143, 151, 166],
    track: [59, 66, 82],
    accent: [136, 192, 208],
    warn: [235, 203, 139],
    crit: [191, 97, 106],
};

/// Dark text on a light background.
pub const LIGHT: Theme = Theme {
    name: "light",
    surface: [244, 244, 242],
    ink: [17, 17, 17],
    ink2: [74, 74, 70],
    ink3: [122, 121, 115],
    track: [217, 216, 211],
    accent: [217, 116, 26],
    warn: [184, 134, 11],
    crit: [198, 40, 40],
};

pub const ALL: [Theme; 6] = [GRIZZLY, CORSAIR, ICE, MONO, NORD, LIGHT];

impl Theme {
    pub const NAMES: [&'static str; 6] = ["grizzly", "corsair", "ice", "mono", "nord", "light"];
}

impl Default for Theme {
    fn default() -> Self {
        GRIZZLY
    }
}

impl fmt::Display for Theme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name)
    }
}

impl FromStr for Theme {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        ALL.into_iter()
            .find(|t| t.name == s)
            .ok_or_else(|| format!("theme must be one of {}", Theme::NAMES.join(", ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip_and_unknown_names_are_refused() {
        assert_eq!(Theme::NAMES.len(), ALL.len());
        for (name, theme) in Theme::NAMES.iter().zip(ALL) {
            assert_eq!(theme.name, *name);
            assert_eq!(name.parse::<Theme>().unwrap(), theme);
            assert_eq!(theme.to_string(), *name);
        }
        assert!("plaid".parse::<Theme>().is_err());
        assert_eq!(Theme::default(), GRIZZLY);
    }

    /// A theme may not make an alarm look like a healthy bar, or hide anything in the background.
    #[test]
    fn every_theme_keeps_the_status_colours_apart() {
        let far = |a: Rgb, b: Rgb| a.iter().zip(b).map(|(x, y)| (i32::from(*x) - i32::from(y)).abs()).sum::<i32>() >= 60;
        for t in ALL {
            assert!(far(t.accent, t.warn), "{}: accent vs warn", t.name);
            assert!(far(t.accent, t.crit), "{}: accent vs crit", t.name);
            assert!(far(t.warn, t.crit), "{}: warn vs crit", t.name);
            for (what, c) in [
                ("ink", t.ink),
                ("ink2", t.ink2),
                ("ink3", t.ink3),
                ("accent", t.accent),
                ("warn", t.warn),
                ("crit", t.crit),
            ] {
                assert!(far(c, t.surface), "{}: {what} vanishes into the background", t.name);
            }
            assert!(far(t.track, t.accent), "{}: track vs accent", t.name);
        }
    }
}
