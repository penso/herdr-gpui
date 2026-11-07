//! Bound Pi's text, image and block counts before retaining a streaming preview.

use acp::v1 as wire;
use serde_json::Value;

use crate::harness::out;

const BLOCKS: usize = 32;
const CUT: &str = "\n… (truncated)";
const EDIT_NOTICE: &str = "Original edit context exceeds live preview limits.";

struct TextBudget(usize);

impl TextBudget {
    fn take(&mut self, text: &str) -> String {
        let value = if text.len() <= self.0 {
            text.to_owned()
        } else if self.0 >= CUT.len() {
            let end = text.floor_char_boundary(self.0 - CUT.len());
            format!("{}{CUT}", &text[..end])
        } else {
            String::new()
        };
        self.0 = self.0.saturating_sub(value.len());
        value
    }
}

/// The bytes of text in `content`; images have a limit of their own.
pub(super) fn size(content: &[wire::ToolCallContent]) -> usize {
    content
        .iter()
        .map(|part| match part {
            wire::ToolCallContent::Content(wire::Content {
                content: wire::ContentBlock::Text(text),
                ..
            }) => text.text.len(),
            wire::ToolCallContent::Diff(diff) => {
                diff.new_text.len() + diff.old_text.as_ref().map_or(0, String::len)
            }
            _ => 0,
        })
        .sum()
}

pub(super) fn initial(name: &str, args: &Value, limit: usize) -> Vec<wire::ToolCallContent> {
    let field = |key: &str| args.get(key).and_then(Value::as_str);
    let mut budget = TextBudget(limit);
    if name == "bash" {
        return field("command")
            .filter(|command| command.lines().nth(1).is_some())
            .map(|command| out::text(&budget.take(command)))
            .into_iter()
            .collect();
    }
    if !matches!(name, "write" | "edit") {
        return Vec::new();
    }
    let path = field("path").unwrap_or_default();
    if path.len() > 4096 || path.chars().any(char::is_control) {
        return vec![out::text(&budget.take(EDIT_NOTICE))];
    }
    // A clipped diff changes its meaning: an omitted new side looks deleted.
    // Retain both sides exactly or show a notice, with space reserved for it.
    budget.0 = budget.0.saturating_sub(EDIT_NOTICE.len());
    let mut omitted = false;
    let mut diff = |old: Option<&str>, new: &str| {
        let size = path
            .len()
            .saturating_add(old.map_or(0, str::len))
            .saturating_add(new.len());
        if size > budget.0 {
            omitted = true;
            return None;
        }
        budget.0 -= size;
        Some(wire::ToolCallContent::Diff(
            wire::Diff::new(path.to_owned(), new.to_owned()).old_text(old.map(str::to_owned)),
        ))
    };
    let mut content: Vec<_> = if name == "write" {
        diff(None, field("content").unwrap_or_default())
            .into_iter()
            .collect()
    } else {
        let edits = args.get("edits").and_then(Value::as_array);
        let content = edits
            .into_iter()
            .flatten()
            .take(BLOCKS)
            .filter_map(|edit| {
                diff(
                    edit.get("oldText").and_then(Value::as_str),
                    edit.get("newText")
                        .and_then(Value::as_str)
                        .unwrap_or_default(),
                )
            })
            .collect();
        omitted |= edits.is_some_and(|edits| edits.len() > BLOCKS);
        content
    };
    if omitted {
        content.push(out::text(&TextBudget(limit).take(EDIT_NOTICE)));
    }
    content
}

pub(super) fn result(
    result: &Value,
    text_limit: usize,
    media_limit: usize,
) -> Option<Vec<wire::ToolCallContent>> {
    let blocks = result.get("content")?.as_array()?;
    let marker = if blocks.len() > BLOCKS {
        "[Additional result blocks omitted]"
    } else {
        CUT
    };
    let marker = &marker[..marker.floor_char_boundary(text_limit)];
    // Keep one notice visible even when an earlier block consumes all text.
    let mut remaining = text_limit.saturating_sub(marker.len());
    let mut omitted = blocks.len() > BLOCKS;
    let mut take = |text: &str| {
        let end = text.floor_char_boundary(remaining);
        remaining -= end;
        omitted |= end < text.len();
        text[..end].to_owned()
    };
    let mut media_left = media_limit;
    let mut content = Vec::new();
    for block in blocks.iter().take(BLOCKS) {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    let text = take(text);
                    if !text.is_empty() {
                        content.push(out::text(&text));
                    }
                }
            }
            Some("image") => {
                let (Some(data), Some(mime)) = (
                    block.get("data").and_then(Value::as_str),
                    block.get("mimeType").and_then(Value::as_str),
                ) else {
                    continue;
                };
                if data.len() <= media_left && mime.len() <= 128 {
                    media_left -= data.len();
                    content.push(wire::ToolCallContent::Content(wire::Content::new(
                        wire::ContentBlock::Image(wire::ImageContent::new(data, mime)),
                    )));
                } else {
                    let text = take("[Image exceeds live preview limits]");
                    if !text.is_empty() {
                        content.push(out::text(&text));
                    }
                }
            }
            _ => {}
        }
    }
    if omitted && !marker.is_empty() {
        content.push(out::text(marker));
    }
    Some(content)
}
