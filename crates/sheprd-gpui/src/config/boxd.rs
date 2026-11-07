//! Saving the `[boxd]` keys Settings edits; the table itself is
//! `sheprd_remote::config::BoxdConfig`.
#[cfg(feature = "boxd")]
use super::{BoxdConfig, BoxdFields, Config};
#[cfg(feature = "boxd")]
use crate::Result;
#[cfg(feature = "boxd")]
use std::path::Path;

#[cfg(feature = "boxd")]
impl Config {
    /// Write the `[boxd]` keys Settings edits to the local override file.
    pub(crate) fn save_boxd(fields: &BoxdFields, existing: &BoxdConfig) -> Result<()> {
        fields.validate(existing)?;
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::save_boxd_path(fields, &local)
    }

    fn save_boxd_path(fields: &BoxdFields, path: &Path) -> Result<()> {
        super::table::save_keys(path, "boxd", &fields.entries())
    }
}

#[cfg(all(test, feature = "boxd"))]
mod tests;
