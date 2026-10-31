//! Themes: what the colour tokens mean in light and in dark (SPEC-2.0 §9.1).
//!
//! A figure names what a colour *means* — `hot`, `muted`, `accent` — and the
//! theme decides what it looks like. Values follow the Okabe–Ito palette,
//! colour-blind safe; the page may override `accent` and `bg` (§15.7).

/// sRGB from a hex literal, at compile time.
const fn hex(rgb: u32) -> [f64; 4] {
    [
        ((rgb >> 16) & 0xff) as f64 / 255.0,
        ((rgb >> 8) & 0xff) as f64 / 255.0,
        (rgb & 0xff) as f64 / 255.0,
        1.0,
    ]
}

const LIGHT: &[(&str, [f64; 4])] = &[
    ("fg", hex(0x1c1b19)),
    ("muted", hex(0x8a857f)),
    ("faint", hex(0xe4e1dc)),
    ("bg", hex(0xfdfdfc)),
    ("accent", hex(0xa0522d)),
    ("hot", hex(0xd55e00)),
    ("cool", hex(0x0072b2)),
    ("positive", hex(0x009e73)),
    ("negative", hex(0xcc3d3d)),
    ("highlight", hex(0xe69f00)),
    ("c1", hex(0x0072b2)),
    ("c2", hex(0xd55e00)),
    ("c3", hex(0x009e73)),
    ("c4", hex(0xcc79a7)),
    ("c5", hex(0xe69f00)),
    ("c6", hex(0x56b4e9)),
    ("c7", hex(0xf0e442)),
    ("c8", hex(0x000000)),
];

const DARK: &[(&str, [f64; 4])] = &[
    ("fg", hex(0xe8e6e3)),
    ("muted", hex(0x7d7872)),
    ("faint", hex(0x34322f)),
    ("bg", hex(0x1a1918)),
    ("accent", hex(0xd98a5f)),
    ("hot", hex(0xff8a3d)),
    ("cool", hex(0x56b4e9)),
    ("positive", hex(0x3ccf9f)),
    ("negative", hex(0xff6b6b)),
    ("highlight", hex(0xffc04d)),
    ("c1", hex(0x56b4e9)),
    ("c2", hex(0xff8a3d)),
    ("c3", hex(0x3ccf9f)),
    ("c4", hex(0xe39bc6)),
    ("c5", hex(0xffc04d)),
    ("c6", hex(0x8fd0f5)),
    ("c7", hex(0xf5ea7a)),
    ("c8", hex(0xe8e6e3)),
];

#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub name: &'static str,
    tokens: Vec<(&'static str, [f64; 4])>,
}

impl Theme {
    pub fn light() -> Self {
        Theme {
            name: "light",
            tokens: LIGHT.to_vec(),
        }
    }

    pub fn dark() -> Self {
        Theme {
            name: "dark",
            tokens: DARK.to_vec(),
        }
    }

    pub fn by_name(name: &str) -> Self {
        if name == "dark" {
            Self::dark()
        } else {
            Self::light()
        }
    }

    /// A token's colour, or none for an unknown token.
    pub fn colour(&self, token: &str) -> Option<[f64; 4]> {
        self.tokens
            .iter()
            .find(|(t, _)| *t == token)
            .map(|(_, c)| *c)
    }

    /// The page may set `accent` and `bg` (§15.7).
    pub fn with(mut self, token: &'static str, colour: [f64; 4]) -> Self {
        match self.tokens.iter_mut().find(|(t, _)| *t == token) {
            Some(slot) => slot.1 = colour,
            None => self.tokens.push((token, colour)),
        }
        self
    }

    pub fn bg(&self) -> [f64; 4] {
        self.colour("bg").unwrap_or([1.0; 4])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG relative luminance and contrast, for the AA checks of §20.
    fn luminance(c: [f64; 4]) -> f64 {
        let lin = |v: f64| {
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
    }

    fn contrast(a: [f64; 4], b: [f64; 4]) -> f64 {
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    #[test]
    fn every_token_exists_in_both_themes() {
        for (token, _) in LIGHT {
            assert!(
                Theme::dark().colour(token).is_some(),
                "{token} missing in dark"
            );
        }
        for token in crate::document::formula::COLOUR_TOKENS {
            assert!(Theme::light().colour(token).is_some(), "{token} missing");
        }
    }

    #[test]
    fn text_meets_aa_and_ink_meets_three_to_one() {
        for theme in [Theme::light(), Theme::dark()] {
            let bg = theme.bg();
            assert!(
                contrast(theme.colour("fg").unwrap(), bg) >= 4.5,
                "{} fg",
                theme.name
            );
            for token in ["accent", "hot", "cool", "positive", "negative", "muted"] {
                let c = contrast(theme.colour(token).unwrap(), bg);
                assert!(c >= 3.0, "{} {token}: {c:.2}", theme.name);
            }
        }
    }
}
