//! The app's colours, as named roles rather than scattered tokens.
//!
//! Palette values follow the yap-a-thon dock's theme rather than being
//! invented: the same hue family as the rest of the desktop so the app does
//! not read as a different product, with NotiFansly's pink as the accent.
//! Dark by default; light available via the header toggle and persisted.

use gpui::{Hsla, hsla};
use std::sync::RwLock;

/// The appearance in force, process-wide so leaf widgets (the text input)
/// can resolve their colours at render time without threading a palette
/// through every listener.
static CURRENT: RwLock<Appearance> = RwLock::new(Appearance::Dark);

pub fn set_appearance(appearance: Appearance) {
    match CURRENT.write() {
        Ok(mut guard) => *guard = appearance,
        Err(poisoned) => *poisoned.into_inner() = appearance,
    }
}

/// The palette for the appearance currently in force.
pub fn appearance() -> Palette {
    let current = match CURRENT.read() {
        Ok(guard) => *guard,
        Err(poisoned) => *poisoned.into_inner(),
    };
    theme_for(current)
}

/// Which palette to use. Persisted in the state file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Appearance {
    Light,
    Dark,
}

impl Appearance {
    pub fn toggle(self) -> Self {
        match self {
            Self::Light => Self::Dark,
            Self::Dark => Self::Light,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    /// Read an appearance by id, falling back to dark. The id comes out of a
    /// state file a person can edit, so an unknown one must not take the
    /// palette down with it.
    pub fn from_id(id: &str) -> Self {
        match id.trim().to_lowercase().as_str() {
            "light" => Self::Light,
            _ => Self::Dark,
        }
    }
}

pub struct Palette {
    pub bg: Hsla,
    pub surface: Hsla,
    pub raised: Hsla,
    pub border: Hsla,
    pub text: Hsla,
    pub muted: Hsla,
    pub accent: Hsla,
    pub accent_dim: Hsla,
    pub on_accent: Hsla,
    pub success: Hsla,
    pub warning: Hsla,
    pub danger: Hsla,
}

// Named font sizes (yap-a-thon): sizes used more than once get a name so a
// change lands everywhere. One step chunkier than the yap-a-thon dock —
// this app is for livestream settings at arm's length, not a dense panel.
pub mod text {
    use gpui::px;
    /// Card / section title.
    pub const TITLE: gpui::Pixels = px(17.0);
    /// Normal reading text.
    pub const BODY: gpui::Pixels = px(14.0);
    /// Supporting text.
    pub const LABEL: gpui::Pixels = px(13.0);
    /// A figure that needs reading, not scanning.
    pub const VALUE: gpui::Pixels = px(20.0);
}

/// The accent pair (dark-panel value, light-panel value). Brand blue
/// #2699f6, checked against both backgrounds the same way yap-a-thon's
/// presets are: the accent is one colour used at a dozen sizes, so it has
/// to clear the body-text bar everywhere. The light-panel value is the
/// same hue deepened (l 0.42 instead of 0.557) so it stays readable on the
/// near-white surfaces.
const ACCENT_ON_DARK: Hsla = Hsla { h: 0.5745, s: 0.92, l: 0.557, a: 1.0 };
const ACCENT_ON_LIGHT: Hsla = Hsla { h: 0.5745, s: 0.90, l: 0.42, a: 1.0 };

pub fn theme_for(appearance: Appearance) -> Palette {
    let dark = appearance == Appearance::Dark;
    Palette {
        bg: if dark {
            hsla(0.625, 0.20, 0.05, 1.0)
        } else {
            hsla(0.600, 0.10, 0.98, 1.0)
        },
        surface: if dark {
            hsla(0.625, 0.18, 0.08, 1.0)
        } else {
            hsla(0.600, 0.08, 1.0, 1.0)
        },
        // A surface one step up, for rows that sit *on* a surface and need
        // to be distinguishable from it without resorting to the accent.
        raised: if dark {
            hsla(0.625, 0.14, 0.14, 1.0)
        } else {
            hsla(0.600, 0.06, 0.94, 1.0)
        },
        border: if dark {
            hsla(0.625, 0.15, 0.18, 1.0)
        } else {
            hsla(0.625, 0.12, 0.88, 1.0)
        },
        text: if dark {
            hsla(0.0, 0.0, 0.93, 1.0)
        } else {
            hsla(0.625, 0.15, 0.10, 1.0)
        },
        muted: if dark {
            hsla(0.625, 0.10, 0.55, 1.0)
        } else {
            hsla(0.625, 0.08, 0.42, 1.0)
        },
        accent: if dark { ACCENT_ON_DARK } else { ACCENT_ON_LIGHT },
        accent_dim: if dark {
            Hsla { h: 0.5745, s: 0.80, l: 0.50, a: 0.14 }
        } else {
            Hsla { h: 0.5745, s: 0.80, l: 0.45, a: 0.12 }
        },
        on_accent: if dark {
            hsla(0.625, 0.20, 0.08, 1.0)
        } else {
            hsla(0.0, 0.0, 0.99, 1.0)
        },
        success: if dark {
            hsla(0.38, 0.70, 0.45, 1.0)
        } else {
            hsla(0.38, 0.62, 0.26, 1.0)
        },
        warning: if dark {
            hsla(0.11, 0.85, 0.55, 1.0)
        } else {
            hsla(0.11, 0.80, 0.30, 1.0)
        },
        danger: if dark {
            hsla(0.0, 0.85, 0.55, 1.0)
        } else {
            hsla(0.0, 0.72, 0.33, 1.0)
        },
    }
}
