//! Saving the `[opensandbox]` keys Settings edits; the table itself is
//! `sheprd_remote::config::OpenSandboxConfig`.
#[cfg(feature = "opensandbox")]
use super::{Config, OpenSandboxConfig, OpenSandboxFields};
#[cfg(feature = "opensandbox")]
use crate::Result;
#[cfg(feature = "opensandbox")]
use std::path::Path;

#[cfg(feature = "opensandbox")]
impl Config {
    /// Write the `[opensandbox]` keys Settings edits to the local override file.
    pub(crate) fn save_opensandbox(
        fields: &OpenSandboxFields,
        existing: &OpenSandboxConfig,
    ) -> Result<()> {
        fields.validate(existing)?;
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::save_opensandbox_path(fields, &local)
    }

    fn save_opensandbox_path(fields: &OpenSandboxFields, path: &Path) -> Result<()> {
        super::table::save_keys(path, "opensandbox", &fields.entries())
    }
}

#[cfg(all(test, feature = "opensandbox"))]
mod tests;
