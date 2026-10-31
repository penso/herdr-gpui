//! Read-only edit previews from supplied text. No filesystem baseline is inferred.

use super::{MAX_PARTS, STRUCTURED_BYTES};
use serde_json::Value;

const LINES: usize = 2_048;
const CELLS: usize = 65_536;
const CONTEXT: usize = 3;
const HEADER: &str = "@@ Provided text @@";

pub(super) struct Shown {
    pub(super) label: String,
    pub(super) language: String,
    pub(super) source: String,
    /// Counts from the generated operations, not a second comparison of text.
    pub(super) counts: Option<(usize, usize)>,
}

fn path(value: &str) -> bool {
    !value.is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
}

fn bounded(text: &str) -> bool {
    text.len() <= STRUCTURED_BYTES && text.lines().take(LINES + 1).count() <= LINES
}

fn shown(path: &str, source: String, counts: (usize, usize)) -> Shown {
    Shown {
        label: format!(
            "{} · provided text",
            crate::notifications::safe_text(path, 240)
        ),
        language: "diff".into(),
        source,
        counts: Some(counts),
    }
}

/// A diff of the supplied fragments, not reconstructed complete files.
pub(super) fn between(pathname: &str, old: &str, new: &str) -> Option<Shown> {
    if !path(pathname) || old.len().checked_add(new.len())? > STRUCTURED_BYTES {
        return None;
    }
    let old: Vec<_> = old.split_inclusive('\n').take(LINES + 1).collect();
    let new: Vec<_> = new.split_inclusive('\n').take(LINES + 1).collect();
    if old.len() > LINES || new.len() > LINES {
        return None;
    }
    let mut source = format!("--- {pathname}\n+++ {pathname}\n");
    if old == new {
        source.push_str("No changes in the supplied text.\n");
        return Some(shown(pathname, source, (0, 0)));
    }
    let prefix = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
    let suffix = old[prefix..]
        .iter()
        .rev()
        .zip(new[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let mut rows = Vec::new();
    rows.extend(
        old[prefix.saturating_sub(CONTEXT)..prefix]
            .iter()
            .map(|line| (' ', *line)),
    );
    let a = &old[prefix..old.len() - suffix];
    let b = &new[prefix..new.len() - suffix];
    if (a.len() + 1).checked_mul(b.len() + 1)? <= CELLS {
        let width = b.len() + 1;
        let mut lengths = vec![0_u16; (a.len() + 1) * width];
        for i in (0..a.len()).rev() {
            for j in (0..b.len()).rev() {
                lengths[i * width + j] = if a[i] == b[j] {
                    lengths[(i + 1) * width + j + 1] + 1
                } else {
                    lengths[(i + 1) * width + j].max(lengths[i * width + j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < a.len() || j < b.len() {
            if i < a.len() && j < b.len() && a[i] == b[j] {
                rows.push((' ', a[i]));
                i += 1;
                j += 1;
            } else if i < a.len()
                && (j == b.len() || lengths[(i + 1) * width + j] >= lengths[i * width + j + 1])
            {
                rows.push(('-', a[i]));
                i += 1;
            } else {
                rows.push(('+', b[j]));
                j += 1;
            }
        }
    } else {
        // A bounded coarse replacement remains truthful; no quadratic work.
        rows.extend(a.iter().map(|line| ('-', *line)));
        rows.extend(b.iter().map(|line| ('+', *line)));
    }
    rows.extend(
        old[old.len() - suffix..]
            .iter()
            .take(CONTEXT)
            .map(|line| (' ', *line)),
    );
    let counts = rows.iter().fold((0, 0), |(added, removed), (kind, _)| {
        (
            added + usize::from(*kind == '+'),
            removed + usize::from(*kind == '-'),
        )
    });
    source.push_str(HEADER);
    source.push('\n');
    let mut at = 0;
    while at < rows.len() {
        if rows[at].0 == ' ' {
            let end = at
                + rows[at..]
                    .iter()
                    .take_while(|(kind, _)| *kind == ' ')
                    .count();
            if end - at > CONTEXT * 2 {
                for &(kind, line) in &rows[at..at + CONTEXT] {
                    row(&mut source, kind, line);
                }
                source.push_str(HEADER);
                source.push('\n');
                at = end - CONTEXT;
            }
        }
        row(&mut source, rows[at].0, rows[at].1);
        at += 1;
    }
    (source.len() <= STRUCTURED_BYTES + LINES * 4 + 8192).then(|| shown(pathname, source, counts))
}

fn row(source: &mut String, kind: char, line: &str) {
    source.push(kind);
    source.push_str(line);
    if !line.ends_with('\n') {
        source.push_str("\n\\ No newline at end of supplied text\n");
    }
}

/// Only an edit tool's first retained input may be interpreted as edit arguments.
pub(super) fn input(text: &str) -> Option<Shown> {
    if !bounded(text) {
        return None;
    }
    let value: Value = serde_json::from_str(text).ok()?;
    let object = value.as_object()?;
    let pathname = object
        .get("file_path")
        .or_else(|| object.get("path"))?
        .as_str()?;
    if !path(pathname)
        || object.contains_key("cmd")
        || object.contains_key("command")
        || object.contains_key("code")
    {
        return None;
    }
    if let (Some(a), Some(b)) = (object.get("file_path"), object.get("path"))
        && a != b
    {
        return None;
    }
    let edits = if let Some(edits) = object.get("edits") {
        if object.contains_key("old_string")
            || object.contains_key("new_string")
            || object.contains_key("oldText")
            || object.contains_key("newText")
        {
            return None;
        }
        let edits = edits.as_array()?;
        if edits.is_empty() || edits.len() > MAX_PARTS {
            return None;
        }
        edits.iter().collect::<Vec<_>>()
    } else {
        vec![&value]
    };
    let mut source = String::new();
    let mut counts = (0, 0);
    for edit in edits {
        if ["path", "file_path"].iter().any(|key| {
            edit.get(*key)
                .is_some_and(|value| value.as_str() != Some(pathname))
        }) {
            return None;
        }
        let (old, new) = if edit.get("old_string").is_some() || edit.get("new_string").is_some() {
            if edit.get("oldText").is_some() || edit.get("newText").is_some() {
                return None;
            }
            (
                edit.get("old_string")?.as_str()?,
                edit.get("new_string")?.as_str()?,
            )
        } else {
            (
                edit.get("oldText")?.as_str()?,
                edit.get("newText")?.as_str()?,
            )
        };
        let fragment = between(pathname, old, new)?;
        let (added, removed) = fragment.counts?;
        counts.0 += added;
        counts.1 += removed;
        let fragment = fragment.source;
        if source.is_empty() {
            source.push_str(&fragment);
        } else {
            // MultiEdit entries share one path. Keep one header pair so the
            // next uncounted fragment cannot mistake file headers for changes.
            source.push_str(fragment.splitn(3, '\n').nth(2)?);
        }
        if source.len() > STRUCTURED_BYTES {
            return None;
        }
    }
    Some(shown(pathname, source, counts))
}

/// A complete patch keeps its own file/hunk metadata; never turn it into a
/// fabricated old/new file. Headerless +/- prose is deliberately not a patch.
pub(super) fn patch(text: &str) -> Option<Shown> {
    if !bounded(text) {
        return None;
    }
    let lines: Vec<_> = text.lines().collect();
    if apply_patch(&lines) || unified(&lines) {
        Some(Shown {
            label: "Reported patch · supplied hunks".into(),
            language: "diff".into(),
            source: text.to_owned(),
            counts: None,
        })
    } else {
        None
    }
}

fn apply_patch(lines: &[&str]) -> bool {
    if lines.first() != Some(&"*** Begin Patch") || lines.last() != Some(&"*** End Patch") {
        return false;
    }
    let mut at = 1;
    let end = lines.len() - 1;
    let mut files = 0;
    while at < end {
        files += 1;
        if files > MAX_PARTS {
            return false;
        }
        let header = lines[at];
        at += 1;
        if let Some(name) = header.strip_prefix("*** Delete File: ") {
            if !path(name) {
                return false;
            }
        } else if let Some(name) = header.strip_prefix("*** Add File: ") {
            if !path(name) {
                return false;
            }
            while at < end && lines[at].starts_with('+') {
                at += 1;
            }
        } else if let Some(name) = header.strip_prefix("*** Update File: ") {
            if !path(name) {
                return false;
            }
            if at < end
                && let Some(target) = lines[at].strip_prefix("*** Move to: ")
            {
                if !path(target) {
                    return false;
                }
                at += 1;
            }
            let mut chunks = 0;
            while at < end {
                if lines[at] == "@@" || lines[at].starts_with("@@ ") {
                    at += 1;
                } else if chunks != 0
                    || !matches!(lines[at].as_bytes().first(), Some(b' ' | b'+' | b'-'))
                {
                    break;
                }
                chunks += 1;
                let mut changed = false;
                let start = at;
                while at < end && matches!(lines[at].as_bytes().first(), Some(b' ' | b'+' | b'-')) {
                    changed |= !lines[at].starts_with(' ');
                    at += 1;
                }
                if at == start || !changed {
                    return false;
                }
                if at < end && lines[at] == "*** End of File" {
                    at += 1;
                    break;
                }
            }
            if chunks == 0 {
                return false;
            }
        } else {
            return false;
        }
    }
    files > 0
}

fn range(raw: &str) -> Option<usize> {
    let (start, count) = raw.split_once(',').unwrap_or((raw, "1"));
    if start.is_empty()
        || count.is_empty()
        || !start.bytes().all(|byte| byte.is_ascii_digit())
        || !count.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let start = start.parse::<u32>().ok()?;
    let count = count.parse::<usize>().ok()?;
    (count <= LINES && (start > 0 || count == 0)).then_some(count)
}

fn hunk(line: &str) -> Option<(usize, usize)> {
    let (ranges, _) = line.strip_prefix("@@ -")?.split_once(" @@")?;
    let (old, new) = ranges.split_once(" +")?;
    Some((range(old)?, range(new)?))
}

fn unified(lines: &[&str]) -> bool {
    let mut at = 0;
    let mut files = 0;
    let mut file_hunks = 0;
    let mut open = false;
    let mut metadata = false;
    // Native Codex prefixes its unified diff with its separately reported path.
    if lines.len() > 1
        && path(lines[0])
        && (lines[1].starts_with("@@ -")
            || lines[1].starts_with("--- ")
            || lines[1].starts_with("diff --git "))
        && !lines[0].starts_with("--- ")
        && !lines[0].starts_with("diff --git ")
    {
        open = lines[1].starts_with("@@ -");
        files = usize::from(open);
        at = 1;
    }
    while at < lines.len() {
        let line = lines[at];
        if line.starts_with("diff --git ") {
            if (open && file_hunks == 0) || metadata {
                return false;
            }
            open = false;
            metadata = true;
            at += 1;
            continue;
        }
        if metadata
            && [
                "index ",
                "new file mode ",
                "deleted file mode ",
                "old mode ",
                "new mode ",
                "similarity index ",
                "rename from ",
                "rename to ",
                "copy from ",
                "copy to ",
            ]
            .iter()
            .any(|prefix| {
                line.strip_prefix(*prefix)
                    .is_some_and(|value| !value.is_empty())
            })
        {
            at += 1;
            continue;
        }
        if let Some(name) = line.strip_prefix("--- ") {
            if (open && file_hunks == 0)
                || !path(name)
                || !lines
                    .get(at + 1)
                    .and_then(|line| line.strip_prefix("+++ "))
                    .is_some_and(path)
            {
                return false;
            }
            files += 1;
            if files > MAX_PARTS {
                return false;
            }
            open = true;
            metadata = false;
            file_hunks = 0;
            at += 2;
            continue;
        }
        let Some((mut old, mut new)) = hunk(line) else {
            return false;
        };
        if !open || old + new == 0 {
            return false;
        }
        file_hunks += 1;
        at += 1;
        let mut changed = false;
        let mut can_note = false;
        while at < lines.len()
            && (old > 0 || new > 0 || lines[at] == "\\ No newline at end of file")
        {
            let line = lines[at];
            let (take_old, take_new) = match line.as_bytes().first() {
                Some(b' ') => (1, 1),
                Some(b'-') => {
                    changed = true;
                    (1, 0)
                }
                Some(b'+') => {
                    changed = true;
                    (0, 1)
                }
                Some(b'\\') if line == "\\ No newline at end of file" && can_note => {
                    can_note = false;
                    at += 1;
                    continue;
                }
                _ => return false,
            };
            let (Some(a), Some(b)) = (old.checked_sub(take_old), new.checked_sub(take_new)) else {
                return false;
            };
            old = a;
            new = b;
            can_note = true;
            at += 1;
        }
        if old != 0 || new != 0 || !changed {
            return false;
        }
    }
    open && !metadata && files > 0 && file_hunks > 0
}
