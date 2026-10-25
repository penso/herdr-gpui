//! One setting at a time, by its dotted key, for the command line: read the
//! effective value, or change the local overrides file. A change is checked
//! the way loading checks the file before it is written, so a value the app
//! would refuse never reaches the disk.
use super::Error;
use super::{Config, DEFAULT_CONFIG, daemon_config_path, daemon_settings, files::write_config};
use crate::Result;
use std::path::Path;
use toml_edit::{Item, TableLike};

/// Where a setting's effective value comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Layer {
    /// `config-gpui.local.toml`, the user's overrides.
    Local,
    /// `config-gpui.toml`, the managed defaults.
    Default,
}

/// A command-line value as TOML: a number, boolean, array, inline table, or
/// quoted string, and anything else as a plain string.
pub(crate) fn parse_value(text: &str) -> toml_edit::Value {
    text.trim()
        .parse::<toml_edit::Value>()
        .unwrap_or_else(|_| toml_edit::Value::from(text))
}

fn lookup<'a>(mut value: &'a toml::Value, key: &str) -> Option<&'a toml::Value> {
    for part in key.split('.') {
        value = value.as_table()?.get(part)?;
    }
    Some(value)
}

impl Config {
    /// The effective value of `key`, overrides over defaults, read without
    /// writing anything. `None` when neither file sets it.
    pub(crate) fn effective_value(key: &str) -> Result<Option<(toml::Value, Layer)>> {
        Self::effective_value_at(&Self::path()?, key)
    }

    pub(super) fn effective_value_at(
        path: &Path,
        key: &str,
    ) -> Result<Option<(toml::Value, Layer)>> {
        let (text, source) = Self::overrides_text(path)?;
        let local: toml::Value = text
            .parse::<toml::Table>()
            .map(toml::Value::Table)
            .map_err(|error| crate::Error::from(error).at_path(&source))?;
        let merged =
            Self::merged([DEFAULT_CONFIG, &text]).map_err(|error| error.at_path(&source))?;
        let layer = if lookup(&local, key).is_some() {
            Layer::Local
        } else {
            Layer::Default
        };
        Ok(lookup(&merged, key).map(|value| (value.clone(), layer)))
    }

    /// The settings as the app would load them now, checked as loading checks
    /// them (the overrides alone, then layered), without writing anything.
    /// Fonts are not resolved: only the app's text system knows them.
    pub(crate) fn check_files() -> Result<Self> {
        Self::check_files_at(&Self::path()?, &daemon_config_path(herdr_client::local_var))
    }

    pub(super) fn check_files_at(path: &Path, daemon: &Path) -> Result<Self> {
        let base = daemon_settings(daemon);
        let (text, source) = Self::overrides_text(path)?;
        Self::parse_over(&text, &base).map_err(|error| error.at_path(&source))?;
        Self::parse_layers([DEFAULT_CONFIG, &text], &base).map_err(|error| error.at_path(&source))
    }

    /// Sets `key` in the local overrides file, keeping everything else in it.
    pub(crate) fn set_key(key: &str, value: toml_edit::Value) -> Result<()> {
        let daemon = daemon_config_path(herdr_client::local_var);
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::edit_key_at(&local, &daemon, key, Some(value)).map(drop)
    }

    /// Removes `key` from the local overrides file; false when it was not set.
    pub(crate) fn unset_key(key: &str) -> Result<bool> {
        let daemon = daemon_config_path(herdr_client::local_var);
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::edit_key_at(&local, &daemon, key, None)
    }

    /// Edits the overrides file at `local`. The edited text must load, and
    /// must not name a key the file did not already get wrong, or the file
    /// is left as it was. Returns whether anything changed.
    pub(super) fn edit_key_at(
        local: &Path,
        daemon: &Path,
        key: &str,
        value: Option<toml_edit::Value>,
    ) -> Result<bool> {
        let parts: Vec<&str> = key.split('.').collect();
        let text = std::fs::read_to_string(local)
            .map_err(|error| crate::Error::from(error).at_path(local))?;
        let mut document = text
            .parse::<toml_edit::DocumentMut>()
            .map_err(|error| crate::Error::from(error).at_path(local))?;
        let changed = match value {
            Some(value) => {
                insert(document.as_table_mut(), &parts, value, key)?;
                true
            }
            None => remove(document.as_table_mut(), &parts),
        };
        if !changed {
            return Ok(false);
        }
        let edited = document.to_string();
        let base = daemon_settings(daemon);
        let checked = Self::parse_over(&edited, &base)?;
        Self::parse_layers([DEFAULT_CONFIG, &edited], &base)?;
        let before = Self::parse_over(&text, &base)
            .map(|config| config.unknown_keys)
            .unwrap_or_default();
        let unknown: Vec<String> = checked
            .unknown_keys
            .into_iter()
            .filter(|key| !before.contains(key))
            .collect();
        if !unknown.is_empty() {
            return Err(Error::UnknownKeys(unknown).into());
        }
        write_config(local, &edited)?;
        Ok(true)
    }
}

fn insert(
    table: &mut dyn TableLike,
    parts: &[&str],
    mut value: toml_edit::Value,
    key: &str,
) -> Result<()> {
    match parts {
        [] => Ok(()),
        [last] => {
            if let Some(previous) = table.get(last).and_then(Item::as_value) {
                *value.decor_mut() = previous.decor().clone();
            }
            table.insert(last, Item::Value(value));
            Ok(())
        }
        [first, rest @ ..] => {
            if !table.contains_key(first) {
                let mut child = toml_edit::Table::new();
                child.set_implicit(true);
                table.insert(first, Item::Table(child));
            }
            let child = table
                .get_mut(first)
                .and_then(Item::as_table_like_mut)
                .ok_or_else(|| Error::KeyNotTable {
                    key: key.to_owned(),
                })?;
            insert(child, rest, value, key)
        }
    }
}

/// Removes the key and the tables it leaves empty.
fn remove(table: &mut dyn TableLike, parts: &[&str]) -> bool {
    match parts {
        [] => false,
        [last] => table.remove(last).is_some(),
        [first, rest @ ..] => {
            let Some(child) = table.get_mut(first).and_then(Item::as_table_like_mut) else {
                return false;
            };
            let removed = remove(child, rest);
            let empty = child.is_empty();
            if removed && empty {
                table.remove(first);
            }
            removed
        }
    }
}
