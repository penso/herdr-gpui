//! Shows or hides one token in the shared `[ui.sidebar.<scope>].rows`.
use super::Error;
use crate::config::{MAX_SIDEBAR_ROWS, SidebarConfigError, SidebarScope};
use toml_edit::{Array, DocumentMut, Item, Value};

/// Showing appends `token` as a row of its own, after writing out upstream's
/// defaults when `rows` is absent so they are not lost; hiding drops every
/// occurrence and the rows that removal leaves empty. Styled inline-table
/// occurrences count as the token, a comment before a dropped row stays with
/// what follows it, and per-agent overrides are never touched.
pub(super) fn edit(
    document: &mut DocumentMut,
    scope: SidebarScope,
    token: &str,
    shown: bool,
) -> Result<(), Error> {
    scope.check(token)?;
    let mut item = document.as_item_mut();
    for key in ["ui", "sidebar", scope.key()] {
        let table = item.as_table_like_mut().ok_or(Error::Table(key))?;
        if !table.contains_key(key) {
            if !shown {
                return Ok(());
            }
            table.insert(key, Item::Table(toml_edit::Table::new()));
        }
        item = table.get_mut(key).ok_or(Error::Table(key))?;
    }
    let table = item.as_table_like_mut().ok_or(Error::Table("sidebar"))?;
    if !table.contains_key("rows") {
        if !shown {
            return Ok(());
        }
        let defaults = scope
            .default_rows()
            .iter()
            .map(|row| row.iter().copied().collect::<Array>())
            .collect::<Array>();
        table.insert("rows", Item::Value(Value::Array(defaults)));
    }
    let rows = table
        .get_mut("rows")
        .and_then(Item::as_array_mut)
        .ok_or(Error::Table("rows"))?;
    let matches = |value: &Value| match value {
        Value::String(name) => name.value() == token,
        Value::InlineTable(styled) => styled.get("token").and_then(Value::as_str) == Some(token),
        _ => false,
    };
    if shown {
        if rows
            .iter()
            .any(|row| row.as_array().is_some_and(|row| row.iter().any(matches)))
        {
            return Ok(());
        }
        if rows.len() >= MAX_SIDEBAR_ROWS {
            return Err(SidebarConfigError::TooManyRows.into());
        }
        rows.push(std::iter::once(token).collect::<Array>());
        return Ok(());
    }
    for index in (0..rows.len()).rev() {
        let Some(Value::Array(row)) = rows.get_mut(index) else {
            continue;
        };
        let before = row.len();
        row.retain(|value| !matches(value));
        if before > 0 && row.is_empty() {
            let removed = rows.remove(index);
            if let Some(comment) = removed
                .decor()
                .prefix()
                .and_then(|raw| raw.as_str())
                .filter(|prefix| prefix.contains('#'))
            {
                keep_comment(rows, index, comment);
            }
        }
    }
    Ok(())
}

fn keep_comment(rows: &mut Array, index: usize, comment: &str) {
    match rows.get_mut(index) {
        Some(next) => {
            let rest = next.decor().prefix().and_then(|raw| raw.as_str());
            let prefix = format!("{comment}{}", rest.unwrap_or(""));
            next.decor_mut().set_prefix(prefix);
        }
        None => {
            let trailing = format!("{comment}{}", rows.trailing().as_str().unwrap_or(""));
            rows.set_trailing(trailing);
        }
    }
}
