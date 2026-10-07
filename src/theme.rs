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

/// Dracula purple on dark.
pub const DRACULA: Theme = Theme {
    name: "dracula",
    surface: [40, 42, 54],
    ink: [248, 248, 242],
    ink2: [201, 201, 208],
    ink3: [98, 114, 164],
    track: [68, 71, 90],
    accent: [189, 147, 249],
    warn: [255, 184, 108],
    crit: [255, 85, 85],
};

/// Gruvbox dark, blue accent.
pub const GRUVBOX: Theme = Theme {
    name: "gruvbox",
    surface: [40, 40, 40],
    ink: [235, 219, 178],
    ink2: [189, 174, 147],
    ink3: [146, 131, 116],
    track: [60, 56, 54],
    accent: [131, 165, 152],
    warn: [250, 189, 47],
    crit: [251, 73, 52],
};

/// Solarized dark.
pub const SOLARIZED: Theme = Theme {
    name: "solarized",
    surface: [0, 43, 54],
    ink: [238, 232, 213],
    ink2: [147, 161, 161],
    ink3: [88, 110, 117],
    track: [7, 54, 66],
    accent: [38, 139, 210],
    warn: [181, 137, 0],
    crit: [220, 50, 47],
};

/// Solarized light.
pub const SOLARIZED_LIGHT: Theme = Theme {
    name: "solarized-light",
    surface: [253, 246, 227],
    ink: [7, 54, 66],
    ink2: [88, 110, 117],
    ink3: [147, 161, 161],
    track: [238, 232, 213],
    accent: [38, 139, 210],
    warn: [181, 137, 0],
    crit: [220, 50, 47],
};

/// Monokai cyan on dark.
pub const MONOKAI: Theme = Theme {
    name: "monokai",
    surface: [39, 40, 34],
    ink: [248, 248, 242],
    ink2: [207, 207, 194],
    ink3: [117, 113, 94],
    track: [62, 61, 50],
    accent: [102, 217, 239],
    warn: [230, 219, 116],
    crit: [249, 38, 114],
};

/// Catppuccin Mocha mauve.
pub const CATPPUCCIN: Theme = Theme {
    name: "catppuccin",
    surface: [30, 30, 46],
    ink: [205, 214, 244],
    ink2: [166, 173, 200],
    ink3: [108, 112, 134],
    track: [49, 50, 68],
    accent: [203, 166, 247],
    warn: [249, 226, 175],
    crit: [243, 139, 168],
};

/// Tokyo Night blue.
pub const TOKYO_NIGHT: Theme = Theme {
    name: "tokyo-night",
    surface: [26, 27, 38],
    ink: [192, 202, 245],
    ink2: [169, 177, 214],
    ink3: [86, 95, 137],
    track: [41, 46, 66],
    accent: [122, 162, 247],
    warn: [224, 175, 104],
    crit: [247, 118, 142],
};

/// Atom One Dark blue.
pub const ONE_DARK: Theme = Theme {
    name: "one-dark",
    surface: [40, 44, 52],
    ink: [171, 178, 191],
    ink2: [157, 165, 180],
    ink3: [92, 99, 112],
    track: [62, 68, 81],
    accent: [97, 175, 239],
    warn: [229, 192, 123],
    crit: [224, 108, 117],
};

/// Rose Pine iris.
pub const ROSE_PINE: Theme = Theme {
    name: "rose-pine",
    surface: [25, 23, 36],
    ink: [224, 222, 244],
    ink2: [144, 140, 170],
    ink3: [110, 106, 134],
    track: [38, 35, 58],
    accent: [196, 167, 231],
    warn: [246, 193, 119],
    crit: [235, 111, 146],
};

/// Everforest aqua on green-grey.
pub const EVERFOREST: Theme = Theme {
    name: "everforest",
    surface: [45, 53, 59],
    ink: [211, 198, 170],
    ink2: [157, 169, 160],
    ink3: [133, 146, 137],
    track: [61, 72, 77],
    accent: [127, 187, 179],
    warn: [219, 188, 127],
    crit: [230, 126, 128],
};

/// Green phosphor on black.
pub const MATRIX: Theme = Theme {
    name: "matrix",
    surface: [0, 0, 0],
    ink: [208, 255, 208],
    ink2: [127, 216, 127],
    ink3: [63, 143, 63],
    track: [15, 42, 15],
    accent: [0, 255, 65],
    warn: [255, 176, 0],
    crit: [255, 48, 48],
};

/// Amber phosphor on black.
pub const AMBER: Theme = Theme {
    name: "amber",
    surface: [0, 0, 0],
    ink: [255, 210, 127],
    ink2: [217, 168, 79],
    ink3: [140, 109, 47],
    track: [42, 31, 8],
    accent: [255, 176, 0],
    warn: [255, 106, 0],
    crit: [255, 32, 32],
};

/// Magenta neon on midnight blue.
pub const CYBERPUNK: Theme = Theme {
    name: "cyberpunk",
    surface: [11, 11, 26],
    ink: [240, 240, 255],
    ink2: [176, 176, 208],
    ink3: [106, 106, 138],
    track: [30, 30, 58],
    accent: [255, 43, 214],
    warn: [255, 230, 0],
    crit: [255, 59, 59],
};

/// Sky blue on deep navy.
pub const OCEAN: Theme = Theme {
    name: "ocean",
    surface: [10, 25, 41],
    ink: [227, 242, 253],
    ink2: [168, 196, 220],
    ink3: [95, 127, 153],
    track: [23, 49, 73],
    accent: [41, 182, 246],
    warn: [255, 179, 0],
    crit: [239, 83, 80],
};

/// Green accent on black.
pub const EMERALD: Theme = Theme {
    name: "emerald",
    surface: [0, 0, 0],
    ink: [255, 255, 255],
    ink2: [185, 184, 176],
    ink3: [122, 121, 115],
    track: [38, 38, 36],
    accent: [46, 204, 113],
    warn: [241, 196, 15],
    crit: [231, 76, 60],
};

/// Purple accent on black.
pub const VIOLET: Theme = Theme {
    name: "violet",
    surface: [0, 0, 0],
    ink: [255, 255, 255],
    ink2: [185, 184, 176],
    ink3: [122, 121, 115],
    track: [38, 38, 36],
    accent: [169, 112, 255],
    warn: [250, 178, 25],
    crit: [208, 59, 59],
};

/// Pink accent on dark plum.
pub const SUNSET: Theme = Theme {
    name: "sunset",
    surface: [26, 16, 32],
    ink: [255, 238, 240],
    ink2: [217, 184, 196],
    ink3: [138, 110, 122],
    track: [46, 26, 51],
    accent: [255, 107, 157],
    warn: [255, 179, 71],
    crit: [255, 59, 59],
};

/// Grey-blue on slate.
pub const SLATE: Theme = Theme {
    name: "slate",
    surface: [15, 23, 42],
    ink: [241, 245, 249],
    ink2: [203, 213, 225],
    ink3: [100, 116, 139],
    track: [30, 41, 59],
    accent: [148, 163, 184],
    warn: [245, 158, 11],
    crit: [239, 68, 68],
};

/// Yellow on black, maximum contrast.
pub const HIGH_CONTRAST: Theme = Theme {
    name: "high-contrast",
    surface: [0, 0, 0],
    ink: [255, 255, 255],
    ink2: [255, 255, 255],
    ink3: [208, 208, 208],
    track: [48, 48, 48],
    accent: [255, 255, 0],
    warn: [255, 140, 0],
    crit: [255, 0, 0],
};

pub const ALL: [Theme; 25] = [
    GRIZZLY,
    CORSAIR,
    ICE,
    MONO,
    NORD,
    LIGHT,
    DRACULA,
    GRUVBOX,
    SOLARIZED,
    SOLARIZED_LIGHT,
    MONOKAI,
    CATPPUCCIN,
    TOKYO_NIGHT,
    ONE_DARK,
    ROSE_PINE,
    EVERFOREST,
    MATRIX,
    AMBER,
    CYBERPUNK,
    OCEAN,
    EMERALD,
    VIOLET,
    SUNSET,
    SLATE,
    HIGH_CONTRAST,
];

impl Theme {
    pub const NAMES: [&'static str; 25] = [
        "grizzly",
        "corsair",
        "ice",
        "mono",
        "nord",
        "light",
        "dracula",
        "gruvbox",
        "solarized",
        "solarized-light",
        "monokai",
        "catppuccin",
        "tokyo-night",
        "one-dark",
        "rose-pine",
        "everforest",
        "matrix",
        "amber",
        "cyberpunk",
        "ocean",
        "emerald",
        "violet",
        "sunset",
        "slate",
        "high-contrast",
    ];
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
