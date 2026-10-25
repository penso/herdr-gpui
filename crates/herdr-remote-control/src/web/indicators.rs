//! Which status marks the lists draw: the set the user picked in Herdr UI,
//! which it sends over the control socket. Kept in the state directory, so
//! the phone keeps the user's set while Herdr UI is closed.

use std::{
    path::{Path, PathBuf},
    sync::{PoisonError, RwLock},
};

/// The file holding the set's config name.
const FILE: &str = "indicators";

/// The sets the phone draws. Herdr UI's animated sets drawn in code
/// (wave, braille, Orca) have no files to share and show as dots.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Indicators {
    /// Herdr's dots: filled, a ring for idle.
    #[default]
    Dots,
    /// Herdr UI's solid marks.
    Solid,
    /// Herdr UI's outline marks.
    Outline,
    /// Each mark on a tile tinted with its colour.
    Tiles,
}

impl Indicators {
    /// The set Herdr UI's config spelling `name` names, or dots for one the
    /// phone cannot draw.
    pub(crate) fn from_config(name: &str) -> Self {
        match name.trim() {
            "solid" => Self::Solid,
            // Herdr UI's retired `rail` drew outline marks.
            "outline" | "rail" => Self::Outline,
            "tiles" => Self::Tiles,
            _ => Self::Dots,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Dots => "dots",
            Self::Solid => "solid",
            Self::Outline => "outline",
            Self::Tiles => "tiles",
        }
    }
}

/// The current set and where it is saved.
#[derive(Debug)]
pub(crate) struct Look {
    file: PathBuf,
    current: RwLock<Indicators>,
}

impl Look {
    /// The set saved in `state_dir`, else dots. Blocking.
    pub(crate) fn load(state_dir: &Path) -> Self {
        let file = state_dir.join(FILE);
        let current = std::fs::read_to_string(&file)
            .map(|name| Indicators::from_config(&name))
            .unwrap_or_default();
        Self {
            file,
            current: RwLock::new(current),
        }
    }

    pub(crate) fn indicators(&self) -> Indicators {
        *self.current.read().unwrap_or_else(PoisonError::into_inner)
    }

    /// Switches to `indicators`, saving it when it changed. Blocking.
    pub(crate) fn set(&self, indicators: Indicators) -> std::io::Result<()> {
        let mut current = self.current.write().unwrap_or_else(PoisonError::into_inner);
        if *current == indicators {
            return Ok(());
        }
        *current = indicators;
        std::fs::write(&self.file, indicators.name())
    }
}
