//! Claude Code without an ACP adapter. `claude` speaks JSON lines on its
//! standard input and output (its `stream-json` mode, which Anthropic's own
//! Agent SDKs drive), so this module stands in for the adapter: a thread
//! that turns the session's ACP into Claude's messages and back. The session
//! cannot tell the difference, and there is no Node process in between.
//!
//! The control messages on the same pipe (`initialize`, `interrupt`,
//! `can_use_tool`, and the setting changes) are not a published interface;
//! they follow Anthropic's Python Agent SDK. Messages of kinds this module
//! does not know are skipped, and `herdr-acp claude --acp` keeps the ACP
//! adapter for when a Claude Code release changes them.

pub(crate) mod background;
mod bridge;
mod permission;
mod settings;
mod tools;

use std::ffi::OsString;

use acp::v1 as wire;
use serde_json::{Value, json};

use crate::harness::{Reach, Translated, out};

/// The program this module starts.
pub(crate) const PROGRAM: &str = "claude";

/// The arguments that start `claude` for one session: a new one under
/// `session`, or the earlier one of that id when `resume` is set.
pub(crate) fn args(session: &str, resume: bool) -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "-p",
        "--input-format",
        "stream-json",
        "--output-format",
        "stream-json",
        "--verbose",
        "--include-partial-messages",
        // Permission questions come here as `can_use_tool` requests.
        "--permission-prompt-tool",
        "stdio",
        // Offers the bypass mode; it is never on unless chosen.
        "--allow-dangerously-skip-permissions",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    args.push(if resume { "--resume" } else { "--session-id" }.into());
    args.push(session.into());
    args
}

/// How the session reaches `claude` running `session`.
pub(crate) fn reach(session: String) -> Box<dyn Reach> {
    Box::new(Translated(bridge::Bridge::new(session)))
}

/// A control response that answers `request` with `response`.
fn control_success(request: &str, response: Value) -> Value {
    json!({
        "type": "control_response",
        "response": {"subtype": "success", "request_id": request, "response": response},
    })
}

/// One block of an ACP prompt as Claude's message content. A file sent as
/// a link is named with `@`, which Claude reads itself.
fn content(block: &wire::ContentBlock) -> Option<Value> {
    let text = |text: String| Some(json!({"type": "text", "text": text}));
    match block {
        wire::ContentBlock::Text(content) => text(content.text.clone()),
        wire::ContentBlock::Image(image) => Some(json!({
            "type": "image",
            "source": {"type": "base64", "media_type": image.mime_type, "data": image.data},
        })),
        wire::ContentBlock::Resource(resource) => match &resource.resource {
            wire::EmbeddedResourceResource::TextResourceContents(file) => {
                text(out::embedded_file(&file.uri, &file.text))
            }
            _ => None,
        },
        wire::ContentBlock::ResourceLink(link) => text(format!("@{}", out::file_path(&link.uri))),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
