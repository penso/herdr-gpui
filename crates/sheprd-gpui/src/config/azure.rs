//! Saving the `[azure]` keys Settings edits; the table itself is
//! `sheprd_remote::config::AzureConfig`.
#[cfg(feature = "azure")]
use super::{AzureConfig, AzureFields, Config};
#[cfg(feature = "azure")]
use crate::Result;
#[cfg(feature = "azure")]
use std::path::Path;

#[cfg(feature = "azure")]
impl Config {
    /// Write the `[azure]` keys Settings edits to the local override file.
    pub(crate) fn save_azure(fields: &AzureFields, existing: &AzureConfig) -> Result<()> {
        fields.validate(existing)?;
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::save_azure_path(fields, &local)
    }

    fn save_azure_path(fields: &AzureFields, path: &Path) -> Result<()> {
        super::table::save_keys(path, "azure", &fields.entries())
    }
}
