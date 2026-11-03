//! Saving the `[boat]` keys Settings edits; the table itself is
//! `sheprd_remote::config::BoatConfig`.
#[cfg(feature = "boat")]
use super::{BoatConfig, BoatFields, Config};
#[cfg(feature = "boat")]
use crate::Result;
#[cfg(feature = "boat")]
use std::path::Path;

#[cfg(feature = "boat")]
impl Config {
    /// Write the `[boat]` keys Settings edits to the local override file.
    pub(crate) fn save_boat(fields: &BoatFields, existing: &BoatConfig) -> Result<()> {
        fields.validate(existing)?;
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::save_boat_path(fields, &local)
    }

    fn save_boat_path(fields: &BoatFields, path: &Path) -> Result<()> {
        super::table::save_keys(path, "boat", &fields.entries())
    }
}

#[cfg(all(test, feature = "boat"))]
mod tests;
