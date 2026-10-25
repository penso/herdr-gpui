//! Targeted shortcut edits use the same lock, validation, and atomic write as
//! other GUI preferences. Reset removes the override, restoring platform and
//! daemon defaults; an empty list explicitly unbinds the command.
use super::{Binding, Config, DEFAULT_CONFIG, Daemon, LOCAL_CONFIG, write_config};
use crate::{Error, Result, controls::COMMANDS};
use std::{fs, io::ErrorKind, path::Path};

impl Config {
    pub(crate) fn save_keybinding(name: &'static str, binding: Option<Binding>) -> Result<()> {
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::save_keybinding_path(name, binding, &local)
    }

    fn save_keybinding_path(name: &str, binding: Option<Binding>, path: &Path) -> Result<()> {
        let result = (|| {
            if !COMMANDS.iter().any(|info| info.name == name) {
                return Err(crate::keymap::Error::UnknownKeybinding(name.into()).into());
            }
            let text = match fs::read_to_string(path) {
                Ok(text) => text,
                Err(error) if error.kind() == ErrorKind::NotFound => LOCAL_CONFIG.into(),
                Err(error) => return Err(error.into()),
            };
            let mut document = text.parse::<toml_edit::DocumentMut>()?;
            let table = document
                .entry("keybindings")
                .or_insert(toml_edit::Item::Table(toml_edit::Table::new()))
                .as_table_like_mut()
                .ok_or(crate::herdr_settings::Error::Table("keybindings"))?;
            match binding {
                None => {
                    table.remove(name);
                }
                Some(binding) => {
                    let mut value = match binding {
                        Binding::One(key) => toml_edit::Value::from(key),
                        Binding::Many(keys) => toml_edit::Value::Array(keys.into_iter().collect()),
                    };
                    if let Some(previous) = table.get(name).and_then(toml_edit::Item::as_value) {
                        *value.decor_mut() = previous.decor().clone();
                    }
                    table.insert(name, toml_edit::Item::Value(value));
                }
            }
            if table.is_empty() {
                document.remove("keybindings");
            }
            let text = document.to_string();
            // Re-read/validate all overrides under the lock, so another window's
            // edit cannot be overwritten or introduce a duplicate shortcut.
            Self::parse_layers([DEFAULT_CONFIG, &text], &Daemon::default())?;
            write_config(path, &text)
        })();
        result.map_err(|error: Error| error.at_path(path))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
