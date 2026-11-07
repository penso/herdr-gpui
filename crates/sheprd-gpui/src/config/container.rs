//! Saving the `[container]` keys Settings edits; the table itself is
//! `sheprd_remote::config::ContainerConfig`.
#[cfg(feature = "container")]
use super::{Config, ContainerConfig, ContainerFields};
#[cfg(feature = "container")]
use crate::Result;
#[cfg(feature = "container")]
use std::path::Path;

#[cfg(feature = "container")]
impl Config {
    /// Write the `[container]` keys Settings edits to the local override file.
    pub(crate) fn save_container(
        fields: &ContainerFields,
        existing: &ContainerConfig,
    ) -> Result<()> {
        fields.validate(existing)?;
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::save_container_path(fields, &local)
    }

    fn save_container_path(fields: &ContainerFields, path: &Path) -> Result<()> {
        super::table::save_keys(path, "container", &fields.entries())
    }
}

#[cfg(all(test, feature = "container"))]
mod tests;
