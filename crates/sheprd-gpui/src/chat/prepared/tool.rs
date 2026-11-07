//! What an opened tool card shows, prepared once per revision: each content
//! item as highlighted source or a diff, beside the exact text it came from.
//! Fields are presentation hints only: nothing is run and no path is opened.

use super::*;
use crate::chat::rich::diff::{Kind, rows};
use std::sync::Arc;

mod diff;

/// Content items of one call read as structure; later ones stay plain text.
pub(super) const MAX_PARTS: usize = 16;
/// The largest payload parsed as arguments or compared as a diff.
const STRUCTURED_BYTES: usize = 64 * 1024;
/// Characters of a path in a preview's label.
const LABEL_PATH: usize = 240;

/// One block of source. Its grammar runs when its card opens, so folded
/// history does not spend the budget message code shares.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Code {
    pub(crate) id: VisualId,
    pub(crate) language: String,
    pub(crate) source: Arc<str>,
}

impl Code {
    fn new(language: &str, source: Arc<str>) -> Self {
        let language = crate::notifications::safe_text(language, 40);
        Self {
            id: VisualId::of(("code", &language, &source)),
            language,
            source,
        }
    }

    /// What a grammar draws this from; plain or oversized source has none.
    fn visual(&self) -> Option<Source> {
        (!self.language.is_empty() && self.source.len() <= crate::chat::rich::MAX_CODE_BYTES).then(
            || Source::Code {
                language: self.language.clone(),
                source: self.source.clone(),
            },
        )
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Preview {
    pub(crate) label: String,
    pub(crate) code: Code,
    /// The earlier text of a change too large to compare, shown above `code`.
    pub(crate) before: Option<Code>,
    /// The retained payload, with what the preview leaves out.
    pub(crate) original: Arc<str>,
}

impl Preview {
    pub(in crate::chat) fn codes(&self) -> impl Iterator<Item = &Code> {
        std::iter::once(&self.code).chain(&self.before)
    }

    pub(super) fn request(&self, visuals: &mut Visuals) {
        for code in self.codes() {
            if let Some(source) = code.visual() {
                visuals.want(code.id, source);
            }
        }
    }

    pub(super) fn request_visible(&self, visuals: &Visuals) {
        for code in self.codes() {
            if let Some(source) = code.visual() {
                visuals.request_visible(code.id, source);
            }
        }
    }

    fn plain(label: String, source: Arc<str>) -> Self {
        Self {
            label,
            code: Code::new("", source.clone()),
            before: None,
            original: source,
        }
    }
}

/// A content item's retained text, as "Copy original" gives it.
pub(crate) fn original(item: &ToolContent) -> Option<String> {
    match item {
        ToolContent::Text { text } => Some(text.clone()),
        ToolContent::Diff { old, new, path } => Some(match old {
            // Each label is a line of its own, between the file's text.
            Some(old) => format!(
                "{path}\n{}\n{old}\n{}\n{new}",
                fl!("chat-diff-before"),
                fl!("chat-diff-after")
            ),
            None => format!("{path}\n{}\n{new}", fl!("chat-diff-written-content")),
        }),
        ToolContent::Terminal { id } => Some(fl!("chat-tool-terminal", id = id.to_string())),
        ToolContent::Image { .. } => None,
    }
}

/// How content item `index` of a `kind` call shows, adding the file changes
/// it reports to `changes`. Only an edit's content counts as changes: a
/// command that prints a patch changed nothing by saying so.
pub(super) fn preview(
    index: usize,
    item: &ToolContent,
    kind: ToolKind,
    changes: &mut Vec<DiffSummary>,
) -> Option<Preview> {
    let structured = index < MAX_PARTS;
    match item {
        ToolContent::Text { text } if !structured => Some(Preview::plain(
            fl!("rich-tools-output"),
            text.as_str().into(),
        )),
        ToolContent::Text { text } => Some(text_preview(index, text, kind, changes)),
        ToolContent::Diff { new, old, path } => {
            let original: Arc<str> = original(item)?.into();
            if structured
                && let Some(old) = old
                && let Some(shown) = diff::between(path, old, new)
            {
                changes.extend(shown.change());
                return Some(shown.into_preview(original));
            }
            let (added, removed) = diff_counts(old.as_deref().unwrap_or(""), new);
            changes.push(DiffSummary {
                path: path.clone(),
                added,
                removed,
            });
            Some(changed_text(path, old.as_deref(), new, original))
        }
        ToolContent::Terminal { id } => Some(Preview::plain(
            fl!("rich-tools-terminal"),
            fl!("chat-tool-terminal", id = id.to_string()).into(),
        )),
        ToolContent::Image { .. } => None,
    }
}

fn text_preview(
    index: usize,
    text: &str,
    kind: ToolKind,
    changes: &mut Vec<DiffSummary>,
) -> Preview {
    let original: Arc<str> = text.into();
    let edit = kind == ToolKind::Edit;
    // Only an edit's first item is what it was asked to do; its later ones
    // are output that may merely look like arguments.
    if edit
        && index == 0
        && let Some(shown) = diff::input(text)
    {
        changes.extend(shown.change());
        return shown.into_preview(original);
    }
    if let Some(shown) = diff::patch(text) {
        if edit {
            changes.extend(patch_changes(&shown.source));
        }
        return shown.into_preview(original);
    }
    if edit
        && index == 0
        && let Some((path, content)) = written(text)
    {
        let added = content.lines().count();
        let preview = changed_text(&path, None, &content, original);
        changes.push(DiffSummary {
            path,
            added,
            removed: 0,
        });
        return preview;
    }
    let (label, language, source) = text_source(&original);
    Preview {
        label,
        code: Code::new(&language, source),
        before: None,
        original,
    }
}

/// A file's new text alone: its earlier text was not reported, or the two
/// are too large to compare. Nothing is said about lines it may have
/// replaced.
fn changed_text(path: &str, old: Option<&str>, new: &str, original: Arc<str>) -> Preview {
    let language = std::path::Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("");
    let path = crate::notifications::safe_text(path, LABEL_PATH);
    Preview {
        label: if old.is_some() {
            fl!("rich-tools-diff-preview-limit", path = path)
        } else {
            fl!("rich-tools-written-content", path = path)
        },
        code: Code::new(language, new.into()),
        before: old.map(|old| Code::new(language, old.into())),
        original,
    }
}

/// The file and text of a whole-file write's arguments.
fn written(text: &str) -> Option<(String, String)> {
    if text.len() > STRUCTURED_BYTES {
        return None;
    }
    let serde_json::Value::Object(mut fields) = serde_json::from_str(text).ok()? else {
        return None;
    };
    let mut string = |field: &str| match fields.remove(field) {
        Some(serde_json::Value::String(text)) => Some(text),
        _ => None,
    };
    let path = string("file_path").or_else(|| string("path"))?;
    Some((path, string("content")?))
}

/// Lines each file of a reported patch adds and removes, or nothing when
/// the patch is too long to read to its end: a partial count would be wrong.
fn patch_changes(source: &str) -> Vec<DiffSummary> {
    let rows = rows(source, usize::MAX);
    if rows.len() < source.lines().count() {
        return Vec::new();
    }
    let mut changes: Vec<DiffSummary> = Vec::new();
    for row in rows {
        let (Some(path), added, removed) = (
            row.file,
            usize::from(row.kind == Kind::Added),
            usize::from(row.kind == Kind::Removed),
        ) else {
            continue;
        };
        if added + removed == 0 {
            continue;
        }
        match changes.iter_mut().find(|change| change.path == path) {
            Some(change) => {
                change.added += added;
                change.removed += removed;
            }
            None => changes.push(DiffSummary {
                path: path.to_owned(),
                added,
                removed,
            }),
        }
    }
    changes
}

/// Reads a text item: known argument fields, a single fence, or as it is.
/// Only explicit fields choose a language; a tool's name does not say what
/// its text is written in.
fn text_source(text: &str) -> (String, String, Arc<str>) {
    if text.len() <= STRUCTURED_BYTES
        && let Ok(serde_json::Value::Object(fields)) = serde_json::from_str(text)
    {
        let string = |field: &str| fields.get(field).and_then(serde_json::Value::as_str);
        for field in ["cmd", "command", "code"] {
            let Some(source) = string(field) else {
                continue;
            };
            let code = field == "code";
            let language = if code {
                string("language").unwrap_or("")
            } else {
                "bash"
            };
            let label = string("description")
                .filter(|description| !description.trim().is_empty())
                .map(|description| crate::notifications::safe_text(description, LABEL_PATH))
                .unwrap_or_else(|| {
                    if code {
                        fl!("rich-tools-source")
                    } else {
                        fl!("rich-tools-command")
                    }
                });
            return (label, language.into(), source.into());
        }
        return (fl!("rich-tools-arguments"), "json".into(), text.into());
    }
    if let Some((open, rest)) = text.trim().split_once('\n')
        && let Some(language) = open.strip_prefix("```")
        && !language.is_empty()
        && language.len() <= 40
        && language
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || "_+-".contains(ch))
        && let Some(source) = rest.strip_suffix("```")
    {
        return (
            fl!("rich-tools-source"),
            language.into(),
            source.trim_end_matches('\n').into(),
        );
    }
    (fl!("rich-tools-output"), String::new(), text.into())
}

#[cfg(test)]
mod tests;
