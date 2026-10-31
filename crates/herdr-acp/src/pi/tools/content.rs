//! Bound Pi's text, image and block counts before retaining a streaming preview.

use acp::v1 as wire;
use serde_json::Value;

use crate::harness::out;

const BLOCKS: usize = 32;
const CUT: &str = "\n… (truncated)";

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
        return vec![out::text(
            "Original edit context exceeds live preview limits.",
        )];
    }
    let mut diff = |old: Option<&str>, new: &str| {
        // Paths are never truncated into a different clickable target.
        if path.len() > budget.0 {
            return None;
        }
        budget.0 -= path.len();
        let old = old.map(|text| budget.take(text));
        Some(wire::ToolCallContent::Diff(
            wire::Diff::new(path.to_owned(), budget.take(new)).old_text(old),
        ))
    };
    if name == "write" {
        return diff(None, field("content").unwrap_or_default())
            .into_iter()
            .collect();
    }
    args.get("edits")
        .and_then(Value::as_array)
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
        .collect()
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
