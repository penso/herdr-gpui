//! Saving the `[coder]` keys Settings edits; the table itself is
//! `sheprd_remote::config::CoderConfig`.
#[cfg(feature = "coder")]
use super::{CoderConfig, CoderFields, Config};
#[cfg(feature = "coder")]
use crate::Result;
#[cfg(feature = "coder")]
use std::path::Path;

#[cfg(feature = "coder")]
impl Config {
    /// Write the `[coder]` keys Settings edits to the local override file.
    pub(crate) fn save_coder(fields: &CoderFields, existing: &CoderConfig) -> Result<()> {
        fields.validate(existing)?;
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::save_coder_path(fields, &local)
    }

    fn save_coder_path(fields: &CoderFields, path: &Path) -> Result<()> {
        super::table::save_keys(path, "coder", &fields.entries())
    }
}

#[cfg(all(test, feature = "coder"))]
mod tests;
