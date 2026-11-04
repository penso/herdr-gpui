//! Saving the `[sprites]` keys Settings edits; the table itself is
//! `sheprd_remote::config::SpritesConfig`.
#[cfg(feature = "sprites")]
use super::{Config, SpritesConfig, SpritesFields};
#[cfg(feature = "sprites")]
use crate::Result;
#[cfg(feature = "sprites")]
use std::path::Path;

#[cfg(feature = "sprites")]
impl Config {
    /// Write the `[sprites]` keys Settings edits to the local override file.
    pub(crate) fn save_sprites(fields: &SpritesFields, existing: &SpritesConfig) -> Result<()> {
        fields.validate(existing)?;
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::save_sprites_path(fields, &local)
    }

    fn save_sprites_path(fields: &SpritesFields, path: &Path) -> Result<()> {
        super::table::save_keys(path, "sprites", &fields.entries())
    }
}

#[cfg(all(test, feature = "sprites"))]
mod tests;
