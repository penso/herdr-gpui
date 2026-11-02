//! The themes starred in the Color Scheme picker. Each window sends its
//! stars and unstars, not its whole list, so a window whose list is stale
//! cannot erase what another window starred.
use super::{Config, Error, LOCAL_CONFIG, files::write_config};
use crate::Result;
use std::{fs, io::ErrorKind, path::Path};

const KEY: &str = "favorite_themes";

/// One click on a theme's star.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FavoriteEdit {
    pub name: String,
    pub starred: bool,
}

/// Applies `edits` in order: a star appends a theme not yet starred, an
/// unstar removes one that is.
pub(crate) fn apply(names: &mut Vec<String>, edits: &[FavoriteEdit]) {
    for edit in edits {
        match (
            edit.starred,
            names.iter().position(|name| *name == edit.name),
        ) {
            (true, None) => names.push(edit.name.clone()),
            (false, Some(index)) => {
                names.remove(index);
            }
            _ => {}
        }
    }
}

/// Queues `edit` to be written, replacing an unwritten edit of the same
/// theme: only the last click on a star counts.
pub(crate) fn queue(edits: &mut Vec<FavoriteEdit>, edit: FavoriteEdit) {
    edits.retain(|queued| queued.name != edit.name);
    edits.push(edit);
}

impl Config {
    /// Applies `edits` to the starred list the local file holds now, under
    /// the config lock, and returns the list it holds afterwards.
    pub(crate) fn save_favorite_themes(edits: &[FavoriteEdit]) -> Result<Vec<String>> {
        let (_lock, local) = Self::prepare_files(&Self::path()?)?;
        Self::save_favorite_themes_path(edits, &local)
    }

    pub(super) fn save_favorite_themes_path(
        edits: &[FavoriteEdit],
        path: &Path,
    ) -> Result<Vec<String>> {
        let result = (|| -> Result<Vec<String>> {
            let text = match fs::read_to_string(path) {
                Ok(text) => text,
                Err(error) if error.kind() == ErrorKind::NotFound => LOCAL_CONFIG.into(),
                Err(error) => return Err(error.into()),
            };
            let mut document = text.parse::<toml_edit::DocumentMut>()?;
            let mut names = Vec::new();
            if let Some(item) = document.get(KEY) {
                for value in item.as_array().ok_or(Error::InvalidFavoriteThemes)? {
                    let name = value.as_str().ok_or(Error::InvalidFavoriteThemes)?;
                    if !names.iter().any(|starred| starred == name) {
                        names.push(name.to_owned());
                    }
                }
            }
            apply(&mut names, edits);
            if names.is_empty() {
                document.remove(KEY);
            } else {
                let mut value = toml_edit::Value::Array(names.iter().map(String::as_str).collect());
                // A comment on the old list stays with the new one.
                if let Some(previous) = document.get(KEY).and_then(toml_edit::Item::as_value) {
                    *value.decor_mut() = previous.decor().clone();
                }
                document[KEY] = toml_edit::Item::Value(value);
            }
            write_config(path, &document.to_string())?;
            Ok(names)
        })();
        result.map_err(|error| error.at_path(path))
    }
}
