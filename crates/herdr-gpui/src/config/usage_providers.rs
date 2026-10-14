//! Which plan-usage services the status bar reads, saved as the
//! `[usage] hide_providers` list so Settings and hand edits stay one thing.
use super::*;

/// One change to the set of usage services that are turned off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProviderEdit {
    Enable(&'static str),
    Disable(&'static str),
    EnableMany([bool; crate::usage::PROVIDER_COUNT]),
    DisableMany([bool; crate::usage::PROVIDER_COUNT]),
}

/// Newly created lists longer than this are written one id per line.
const INLINE_LIMIT: usize = 4;

/// The `hide_providers` value after `edit`, or `None` when nothing is left
/// hidden and no comments would be lost by removing the key. Ids this build
/// does not know, and the existing array's formatting, are kept.
pub(super) fn hide_providers(
    document: &toml_edit::DocumentMut,
    edit: ProviderEdit,
) -> Result<Option<toml_edit::Value>> {
    let existing = match document.get("usage") {
        None => None,
        Some(usage) => {
            let usage = usage.as_table_like().ok_or(Error::InvalidUsageTable)?;
            match usage.get("hide_providers") {
                None => None,
                Some(item) => {
                    let array = item.as_array().ok_or(Error::InvalidHideProviders)?;
                    if array.iter().any(|id| id.as_str().is_none()) {
                        return Err(Error::InvalidHideProviders);
                    }
                    Some(array)
                }
            }
        }
    };
    let mut hidden = existing.cloned().unwrap_or_default();
    match edit {
        ProviderEdit::Enable(id) => remove_hidden(&mut hidden, |value| value.as_str() == Some(id)),
        ProviderEdit::Disable(id) => add_hidden(&mut hidden, id),
        ProviderEdit::EnableMany(selected) => {
            remove_hidden(&mut hidden, |value| {
                crate::usage::providers()
                    .zip(selected)
                    .any(|(provider, selected)| selected && value.as_str() == Some(provider.id()))
            });
        }
        ProviderEdit::DisableMany(selected) => {
            for (provider, selected) in crate::usage::providers().zip(selected) {
                if selected {
                    add_hidden(&mut hidden, provider.id());
                }
            }
        }
    }
    if hidden.is_empty()
        && !has_comment(Some(hidden.trailing()))
        && !has_comment(hidden.decor().prefix())
        && !has_comment(hidden.decor().suffix())
    {
        return Ok(None);
    }
    if existing.is_none() && hidden.len() > INLINE_LIMIT {
        for id in hidden.iter_mut() {
            id.decor_mut().set_prefix("\n  ");
        }
        hidden.set_trailing("\n");
        hidden.set_trailing_comma(true);
    }
    Ok(Some(toml_edit::Value::Array(hidden)))
}

fn add_hidden(hidden: &mut toml_edit::Array, id: &str) {
    if !hidden.iter().any(|value| value.as_str() == Some(id)) {
        hidden.push(id);
    }
}

fn has_comment(raw: Option<&toml_edit::RawString>) -> bool {
    raw.and_then(toml_edit::RawString::as_str)
        .is_some_and(|text| text.contains('#'))
}

fn remove_hidden(hidden: &mut toml_edit::Array, mut remove: impl FnMut(&toml_edit::Value) -> bool) {
    let mut index = 0;
    let mut comments = String::new();
    while index < hidden.len() {
        if hidden.get(index).is_some_and(&mut remove) {
            let removed = hidden.remove(index);
            // A prefix can describe the whole list or the preceding value,
            // not just the removed id. Keep comment-bearing decorations in
            // order, moving them to the next value or the array's trailing.
            for raw in [removed.decor().prefix(), removed.decor().suffix()] {
                if has_comment(raw)
                    && let Some(text) = raw.and_then(toml_edit::RawString::as_str)
                {
                    comments.push_str(text);
                }
            }
            continue;
        }
        if !comments.is_empty()
            && let Some(value) = hidden.get_mut(index)
        {
            if let Some(prefix) = value
                .decor()
                .prefix()
                .and_then(toml_edit::RawString::as_str)
            {
                comments.push_str(prefix);
            }
            value.decor_mut().set_prefix(std::mem::take(&mut comments));
        }
        index += 1;
    }
    if !comments.is_empty() {
        if let Some(trailing) = hidden.trailing().as_str() {
            comments.push_str(trailing);
        }
        hidden.set_trailing(comments);
    }
}
