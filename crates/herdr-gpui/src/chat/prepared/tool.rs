//! Bounded tool payloads, prepared once per transcript revision. Fields are
//! presentation hints only: no script is evaluated and no path is opened.

use super::*;
use std::sync::Arc;

mod diff;

pub(crate) const COMPACT_LINES: usize = 4;
pub(crate) const COMPACT_LOCATIONS: usize = 3;
pub(super) const MAX_PARTS: usize = 16;
const STRUCTURED_BYTES: usize = 64 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Code {
    pub(crate) id: VisualId,
    pub(crate) language: String,
    pub(crate) source: Arc<str>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Preview {
    pub(crate) label: String,
    pub(crate) code: Code,
    /// Only a structured diff has a before side; it is disclosed on expansion.
    pub(crate) before: Option<Code>,
    /// Exact retained input/output, including metadata omitted from the preview.
    pub(crate) original: Arc<str>,
}

impl Preview {
    pub(crate) fn visuals(&self) -> impl Iterator<Item = VisualId> + '_ {
        std::iter::once(self.code.id).chain(self.before.iter().map(|code| code.id))
    }
}

pub(crate) fn label(title: &str, summary: Option<&ToolSummary>) -> String {
    let title = tool_label(title);
    let title = match title {
        "exec" | "Tool exec" | "exec_command" | "functions.exec" | "Bash" | "bash" => summary
            .and_then(|summary| summary.previews.first())
            .filter(|preview| preview.code.language == "bash")
            .and_then(|preview| {
                preview
                    .code
                    .source
                    .lines()
                    .find(|line| !line.trim().is_empty())
            })
            .unwrap_or(title),
        _ => title,
    };
    crate::notifications::safe_text(title, 300)
}

pub(crate) fn original(item: &ToolContent) -> Option<String> {
    match item {
        ToolContent::Text { text } => Some(text.clone()),
        ToolContent::Diff { old, new, path } => Some(match old {
            Some(old) => format!("{path}\nBefore:\n{old}\nAfter:\n{new}"),
            None => format!("{path}\nWritten content (previous content unavailable):\n{new}"),
        }),
        ToolContent::Terminal { id } => Some(format!(
            "Terminal {id}: output is available in the program's terminal view."
        )),
        ToolContent::Image { .. } => None,
    }
}

pub(crate) fn previews(
    content: &[ToolContent],
    kind: ToolKind,
    visuals: &mut Visuals,
) -> (Vec<Preview>, Vec<DiffSummary>) {
    let mut diffs = Vec::new();
    let previews = content
        .iter()
        .take(MAX_PARTS)
        .enumerate()
        .filter_map(|(index, item)| {
            let preview = match item {
                ToolContent::Text { text } => {
                    let original: Arc<str> = text.as_str().into();
                    if let Some(shown) = (kind == ToolKind::Edit && index == 0)
                        .then(|| diff::input(text))
                        .flatten()
                        .or_else(|| diff::patch(text))
                    {
                        return Some(Preview {
                            label: shown.label,
                            code: code(&shown.language, shown.source.into(), visuals),
                            before: None,
                            original,
                        });
                    }
                    let (label, language, source) = text_source(&original);
                    let source = if source.as_ref() == original.as_ref() {
                        original.clone()
                    } else {
                        source
                    };
                    Preview {
                        label,
                        code: code(&language, source, visuals),
                        before: None,
                        original,
                    }
                }
                ToolContent::Diff { new, old, path } => {
                    let original: Arc<str> = original(item)?.into();
                    if let Some(old) = old
                        && let Some(shown) = diff::between(path, old, new)
                    {
                        if let Some((added, removed)) = shown.counts {
                            diffs.push(DiffSummary {
                                path: path.clone(),
                                added,
                                removed,
                            });
                        }
                        return Some(Preview {
                            label: shown.label,
                            code: code(&shown.language, shown.source.into(), visuals),
                            before: None,
                            original,
                        });
                    }
                    let language = std::path::Path::new(path)
                        .extension()
                        .and_then(|extension| extension.to_str())
                        .unwrap_or("");
                    Preview {
                        label: {
                            format!(
                                "{} · {}",
                                crate::notifications::safe_text(path, 240),
                                if old.is_some() {
                                    "provided text exceeds diff preview limits"
                                } else {
                                    "written content; previous content unavailable"
                                }
                            )
                        },
                        code: code(language, new.as_str().into(), visuals),
                        before: old
                            .as_ref()
                            .map(|old| code(language, old.as_str().into(), visuals)),
                        original,
                    }
                }
                ToolContent::Terminal { id } => {
                    let source: Arc<str> = format!(
                        "Terminal {id}: output is available in the program's terminal view."
                    )
                    .into();
                    Preview {
                        label: "Terminal".into(),
                        code: code("", source.clone(), visuals),
                        before: None,
                        original: source,
                    }
                }
                ToolContent::Image { .. } => return None,
            };
            Some(preview)
        })
        .collect();
    (previews, diffs)
}

fn code(language: &str, source: Arc<str>, visuals: &mut Visuals) -> Code {
    let language = crate::notifications::safe_text(language, 40);
    let id = VisualId::of(("code", &language, &source));
    if !language.is_empty() && source.len() <= crate::chat::rich::MAX_CODE_BYTES {
        visuals.want(
            id,
            Source::Code {
                language: language.clone(),
                source: source.clone(),
            },
        );
    }
    Code {
        id,
        language,
        source,
    }
}

fn text_source(text: &str) -> (String, String, Arc<str>) {
    // Only explicit structured fields select a language. In particular, a
    // generic "Tool script" title does not establish that text is JavaScript.
    if text.len() <= STRUCTURED_BYTES
        && let Ok(serde_json::Value::Object(fields)) = serde_json::from_str(text)
    {
        for field in ["cmd", "command", "code"] {
            if let Some(source) = fields.get(field).and_then(serde_json::Value::as_str) {
                let language = if field == "code" {
                    fields
                        .get("language")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("")
                } else {
                    "bash"
                };
                let label = fields
                    .get("description")
                    .and_then(serde_json::Value::as_str)
                    .filter(|description| !description.trim().is_empty())
                    .map(|description| crate::notifications::safe_text(description, 240))
                    .unwrap_or_else(|| if field == "code" { "Source" } else { "Command" }.into());
                return (label, language.into(), source.into());
            }
        }
        // Structured arguments without a command remain visible JSON; do not
        // derive a guessed command from paths, arbitrary keys, or tool names.
        return ("Arguments".into(), "json".into(), text.into());
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
            "Source".into(),
            language.into(),
            source.trim_end_matches('\n').into(),
        );
    }
    ("Output / source".into(), String::new(), text.into())
}

#[cfg(test)]
mod tests;
