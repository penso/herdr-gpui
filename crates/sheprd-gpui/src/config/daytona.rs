//! Saving the `[daytona]` keys Settings edits; the table itself is
//! `sheprd_remote::config::DaytonaConfig`.
#[cfg(feature = "daytona")]
use super::{Config, DaytonaConfig, DaytonaFields};
#[cfg(feature = "daytona")]
use crate::Result;
#[cfg(feature = "daytona")]
use std::path::Path;

#[cfg(feature = "daytona")]
impl Config {
    /// Write the `[daytona]` keys Settings edits to the local override file.
    pub(crate) fn save_daytona(fields: &DaytonaFields, existing: &DaytonaConfig) -> Result<()> {
        fields.validate(existing)?;
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::save_daytona_path(fields, &local)
    }

    fn save_daytona_path(fields: &DaytonaFields, path: &Path) -> Result<()> {
        super::table::save_keys(path, "daytona", &fields.entries())
    }
}

#[cfg(all(test, feature = "daytona"))]
mod tests;
