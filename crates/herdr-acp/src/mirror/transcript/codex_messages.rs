//! Per-content provenance separates harness updates from a person's prompt.
//! Embedded raster bytes are sufficient for images; paths and URLs stay inert.

use herdr_chat_protocol::{Author, EntryId, EntryKind, Image, MAX_IMAGE_DATA};
use serde_json::Value;

use super::{BLOCKS, Projection, Role, TEXT, notices, string, text_block};

/// Larger records are useful only for inline image bytes, never arbitrary
/// tool output or text. The caller still enforces the bounded tail size.
pub(super) fn image_record(record: &Value) -> bool {
    let payload = &record["payload"];
    string(record, "type") == "response_item"
        && string(payload, "type") == "message"
        && matches!(string(payload, "role"), "user" | "assistant")
        && payload["content"].as_array().is_some_and(|blocks| {
            blocks
                .iter()
                .take(BLOCKS)
                .any(|block| string(block, "type") == "input_image")
        })
}

impl Projection {
    pub(super) fn codex_message(&mut self, base: EntryId, record: &Value) {
        let payload = &record["payload"];
        let role = if string(payload, "channel") == "analysis" {
            Some(Role::Thought)
        } else {
            Role::of(string(payload, "role"))
        };
        let Some(role) = role else {
            return;
        };
        let Some(blocks) = payload["content"].as_array() else {
            let content = self.text(&payload["content"]);
            self.message(base, role, content);
            return;
        };
        self.transcript.truncated |= blocks.len() > BLOCKS;
        let mut pending: Option<(EntryId, String)> = None;
        let flush = |this: &mut Self, pending: &mut Option<(EntryId, String)>| {
            if let Some((id, content)) = pending.take() {
                this.message(id, role, content);
            }
        };
        let mut index = 0;
        while index < blocks.len().min(BLOCKS) {
            let id = EntryId(base.0 + index as u64);
            if role == Role::User
                && let Some(attachment) = attachment(payload, blocks, index)
                && index + attachment.consumed <= BLOCKS
            {
                flush(self, &mut pending);
                self.transcript.truncated |= attachment.source.len() > TEXT;
                self.push(
                    id,
                    notices::source(
                        format!("Image attachment · {}", attachment.name),
                        &attachment.source,
                    ),
                );
                self.push(EntryId(id.0 + 1), picture(attachment.image, Author::User));
                if let Some(caption) = attachment.caption {
                    let caption = self.clip(caption, TEXT);
                    self.message(EntryId(id.0 + 3), role, caption);
                }
                index += attachment.consumed;
                continue;
            }
            let block = &blocks[index];
            match string(block, "type") {
                _ if text_block(block) => {
                    let content = string(block, "text");
                    if role == Role::User
                        && let Some(reply) = self.question_reply(record, content)
                    {
                        flush(self, &mut pending);
                        self.push(id, reply);
                    } else if role == Role::User
                        && let Some(label) = context_label(record, index, blocks.len())
                    {
                        flush(self, &mut pending);
                        self.transcript.truncated |= content.len() > TEXT;
                        self.push(id, notices::source(label.into(), content));
                    } else if !content.is_empty() {
                        let (_, pending) = pending.get_or_insert_with(|| (id, String::new()));
                        if !pending.is_empty() && pending.len() < TEXT {
                            pending.push('\n');
                        }
                        let remaining = TEXT.saturating_sub(pending.len());
                        pending.push_str(&self.clip(content, remaining));
                    }
                }
                "input_image" => {
                    if let Some(author) = role.author() {
                        flush(self, &mut pending);
                        self.push(id, picture(embedded_image(block), author));
                    }
                }
                _ => {}
            }
            index += 1;
        }
        flush(self, &mut pending);
    }
}

fn context_label(record: &Value, index: usize, count: usize) -> Option<&'static str> {
    let payload = &record["payload"];
    if let Some((kind, "harness")) = provenance(payload, index, count) {
        return Some(if kind == "environments.environment_context" {
            "Environment context updated"
        } else {
            "System context"
        });
    }
    let metadata = &payload["internal_chat_message_metadata_passthrough"];
    let kinds = metadata["content_item_kinds"].as_array()?;
    // Older native context records predate per-slot provenance. Their exact
    // typed kind identifies the producer; pasted XML instead has user.text.
    // Present but malformed or explicit user provenance cannot use this path.
    if metadata.get("content_item_metadata").is_some()
        || record["metadata"]["client_authored"].as_bool() == Some(true)
        || kinds.len() != count
    {
        return None;
    }
    match kinds.get(index)?.as_str()? {
        "environments.environment_context" => Some("Environment context updated"),
        "agents_md.instructions" => Some("System context"),
        _ => None,
    }
}

/// Both native arrays describe the same content slots. A malformed
/// envelope cannot establish a different author by a coincidental text match.
fn provenance(payload: &Value, index: usize, count: usize) -> Option<(&str, &str)> {
    let metadata = &payload["internal_chat_message_metadata_passthrough"];
    let kinds = metadata["content_item_kinds"].as_array()?;
    let items = metadata["content_item_metadata"].as_array()?;
    if kinds.len() != count || items.len() != count {
        return None;
    }
    Some((
        kinds.get(index)?.as_str()?,
        items.get(index)?["provenance"]["type"].as_str()?,
    ))
}

fn embedded_image(block: &Value) -> Option<Image> {
    if string(block, "type") != "input_image" {
        return None;
    }
    let url = string(block, "image_url").strip_prefix("data:")?;
    let (mime, data) = url.split_once(";base64,")?;
    if data.len() > MAX_IMAGE_DATA {
        return None;
    }
    Image::new(mime, data.to_owned())
}

fn picture(image: Option<Image>, author: Author) -> EntryKind {
    match image {
        Some(image) => EntryKind::Image { author, image },
        None => notices::source(
            "Image unavailable".into(),
            "The embedded image data is missing, invalid, too large, or unsupported. Supported formats are PNG, JPEG, GIF, and WebP.",
        ),
    }
}

struct Attachment<'a> {
    caption: Option<&'a str>,
    consumed: usize,
    image: Option<Image>,
    name: &'a str,
    source: String,
}

fn attachment<'a>(payload: &Value, blocks: &'a [Value], index: usize) -> Option<Attachment<'a>> {
    let opening = blocks.get(index)?;
    let image = blocks.get(index + 1)?;
    let closing = blocks.get(index + 2)?;
    if provenance(payload, index, blocks.len()) != Some(("user.text", "user"))
        || provenance(payload, index + 1, blocks.len()) != Some(("user.image", "user"))
        || provenance(payload, index + 2, blocks.len()) != Some(("user.text", "user"))
        || string(opening, "type") != "input_text"
        || string(image, "type") != "input_image"
        || string(closing, "type") != "input_text"
        || string(closing, "text") != "</image>"
    {
        return None;
    }
    let opening = string(opening, "text");
    let wrapper = opening.strip_prefix("<image name=")?.strip_suffix('>')?;
    let (name, path) = wrapper.split_once(" path=\"")?;
    let path = path.strip_suffix('"')?;
    let number = name.strip_prefix("[Image #")?.strip_suffix(']')?;
    if number.is_empty()
        || !number.bytes().all(|byte| byte.is_ascii_digit())
        || name.len() > 64
        || path.is_empty()
        || path.contains(['\n', '\r', '"', '<', '>'])
    {
        return None;
    }
    let image = embedded_image(image);
    let caption = blocks.get(index + 3).filter(|block| {
        string(block, "type") == "input_text"
            && provenance(payload, index + 3, blocks.len()) == Some(("user.text", "user"))
            && string(block, "text").starts_with(name)
    });
    let mut source = format!("{opening}\n</image>");
    if let Some(caption) = caption {
        source.push('\n');
        source.push_str(string(caption, "text"));
    }
    Some(Attachment {
        caption: caption.map(|block| &string(block, "text")[name.len()..]),
        consumed: if caption.is_some() { 4 } else { 3 },
        image,
        name,
        source,
    })
}
