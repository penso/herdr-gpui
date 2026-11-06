//! Finds the GUI that `herdr-gpui-launcher.exe`, the windowed Windows launcher, starts.

mod error;

use std::path::{Path, PathBuf};

pub use error::{Error, Result};

/// The GUI and CLI executable, installed beside the launcher.
const GUI_EXE: &str = "sheprd.exe";

/// The `herdr-gpui.exe` in the launcher's own directory.
///
/// # Errors
///
/// [`Error::NoParent`] when `launcher` has no parent directory.
pub fn gui_path(launcher: &Path) -> Result<PathBuf> {
    let directory = launcher.parent().ok_or_else(|| Error::NoParent {
        launcher: launcher.to_owned(),
    })?;
    let current = directory.join(GUI_EXE);
    let legacy = directory.join("herdr-gpui.exe");
    Ok(if !current.is_file() && legacy.is_file() {
        legacy
    } else {
        current
    })
}

#[cfg(test)]
mod tests;
