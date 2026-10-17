//! A chat view of a Claude Code session, read from the JSONL transcript whose
//! path the hooks report. Only the tail is read, and every entry is clipped,
//! so a long session costs a bounded amount per request. Entries are display
//! data; clients must render them as text, never as markup.

use crate::{Error, Result};
use serde::Serialize;
use serde_json::Value;
use std::{
    fs::File,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};

const MAX_TAIL: u64 = 4 * 1024 * 1024;
const MAX_TEXT: usize = 8 * 1024;
const MAX_RESULT: usize = 2 * 1024;
const MAX_SUMMARY: usize = 300;
pub const DEFAULT_ENTRIES: usize = 200;
pub const MAX_ENTRIES: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Entry {
    User {
        text: String,
        at: Option<String>,
    },
    Assistant {
        text: String,
        at: Option<String>,
    },
    ToolUse {
        id: Option<String>,
        name: String,
        summary: String,
        at: Option<String>,
    },
    ToolResult {
        tool_use_id: Option<String>,
        text: String,
        is_error: bool,
        at: Option<String>,
    },
}

/// The last `limit` entries of the transcript at `path`.
pub fn read(path: &Path, limit: usize) -> Result<Vec<Entry>> {
    let tail = read_tail(path).map_err(|error| match error.kind() {
        io::ErrorKind::NotFound => Error::NoTranscript,
        _ => Error::Io(error),
    })?;
    let mut entries = parse(&tail);
    let excess = entries.len().saturating_sub(limit.min(MAX_ENTRIES));
    entries.drain(..excess);
    Ok(entries)
}

fn read_tail(path: &Path) -> io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let start = file.metadata()?.len().saturating_sub(MAX_TAIL);
    file.seek(SeekFrom::Start(start))?;
    let mut tail = Vec::new();
    file.take(MAX_TAIL).read_to_end(&mut tail)?;
    if start > 0 {
        // The first line was cut by the seek; it would only parse as garbage.
        let first = tail
            .iter()
            .position(|&byte| byte == b'\n')
            .map_or(tail.len(), |at| at + 1);
        tail.drain(..first);
    }
    Ok(tail)
}

fn parse(tail: &[u8]) -> Vec<Entry> {
    let mut entries = Vec::new();
    for line in tail.split(|&byte| byte == b'\n') {
        let Ok(record) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        let flag = |name| record.get(name).and_then(Value::as_bool).unwrap_or(false);
        // Subagent turns and injected meta messages are not the conversation.
        if flag("isSidechain") || flag("isMeta") {
            continue;
        }
        let assistant = match record.get("type").and_then(Value::as_str) {
            Some("user") => false,
            Some("assistant") => true,
            _ => continue,
        };
        let at = record
            .get("timestamp")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let message = |text: &str| {
            let text = clip(text, MAX_TEXT);
            if assistant {
                Entry::Assistant {
                    text,
                    at: at.clone(),
                }
            } else {
                Entry::User {
                    text,
                    at: at.clone(),
                }
            }
        };
        match record.pointer("/message/content") {
            Some(Value::String(text)) if !text.trim().is_empty() => entries.push(message(text)),
            Some(Value::Array(blocks)) => {
                for block in blocks {
                    let field = |name| block.get(name).and_then(Value::as_str);
                    match field("type") {
                        Some("text") => {
                            if let Some(text) = field("text").filter(|text| !text.trim().is_empty())
                            {
                                entries.push(message(text));
                            }
                        }
                        Some("tool_use") => {
                            let name = field("name").unwrap_or_default().to_owned();
                            let summary =
                                tool_summary(&name, block.get("input").unwrap_or(&Value::Null));
                            entries.push(Entry::ToolUse {
                                id: field("id").map(str::to_owned),
                                name,
                                summary,
                                at: at.clone(),
                            });
                        }
                        Some("tool_result") => entries.push(Entry::ToolResult {
                            tool_use_id: field("tool_use_id").map(str::to_owned),
                            text: clip(&result_text(block.get("content")), MAX_RESULT),
                            is_error: block
                                .get("is_error")
                                .and_then(Value::as_bool)
                                .unwrap_or(false),
                            at: at.clone(),
                        }),
                        // Thinking, images, and unknown blocks are not shown.
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    entries
}

fn result_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

/// One line saying what a tool call does, for cards and the activity feed.
pub fn tool_summary(name: &str, input: &Value) -> String {
    let field = |key| input.get(key).and_then(Value::as_str);
    let summary = match name {
        "Bash" => field("command"),
        "Read" | "Write" | "Edit" | "MultiEdit" | "NotebookEdit" => {
            field("file_path").or(field("notebook_path"))
        }
        "Grep" | "Glob" => field("pattern"),
        "WebFetch" => field("url"),
        "WebSearch" => field("query"),
        "Task" | "Agent" => field("description"),
        _ => None,
    };
    let summary = summary.or_else(|| {
        // Unknown tools, MCP tools included: the first string argument.
        input.as_object()?.values().find_map(Value::as_str)
    });
    clip(summary.unwrap_or_default(), MAX_SUMMARY)
}

pub(crate) fn clip(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        return text.to_owned();
    }
    let mut end = limit;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    fn lines(records: &[Value]) -> Vec<u8> {
        records
            .iter()
            .map(|record| format!("{record}\n"))
            .collect::<String>()
            .into_bytes()
    }

    #[test]
    fn turns_a_transcript_into_chat_entries() {
        let tail = lines(&[
            json!({"type": "summary", "summary": "x"}),
            json!({"type": "user", "timestamp": "t1", "message": {"role": "user", "content": "fix the build"}}),
            json!({"type": "assistant", "timestamp": "t2", "message": {"role": "assistant", "content": [
                {"type": "thinking", "thinking": "hidden"},
                {"type": "text", "text": "Running the tests."},
                {"type": "tool_use", "id": "tu1", "name": "Bash", "input": {"command": "cargo test", "description": "Run tests"}}
            ]}}),
            json!({"type": "user", "timestamp": "t3", "message": {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "tu1", "is_error": true, "content": [{"type": "text", "text": "1 failed"}]}
            ]}}),
            json!({"type": "user", "isMeta": true, "message": {"content": "caveat"}}),
            json!({"type": "assistant", "isSidechain": true, "message": {"content": "subagent"}}),
        ]);
        let mut tail = tail;
        tail.extend(b"not json\n");
        assert_eq!(
            parse(&tail),
            [
                Entry::User {
                    text: "fix the build".into(),
                    at: Some("t1".into())
                },
                Entry::Assistant {
                    text: "Running the tests.".into(),
                    at: Some("t2".into())
                },
                Entry::ToolUse {
                    id: Some("tu1".into()),
                    name: "Bash".into(),
                    summary: "cargo test".into(),
                    at: Some("t2".into())
                },
                Entry::ToolResult {
                    tool_use_id: Some("tu1".into()),
                    text: "1 failed".into(),
                    is_error: true,
                    at: Some("t3".into())
                },
            ]
        );
    }

    #[test]
    fn reads_only_the_tail_and_keeps_the_last_entries() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.jsonl");
        let mut file = File::create(&path).unwrap();
        let filler = "x".repeat(1024);
        // Well past the tail limit, so the first line read is a partial one.
        for _ in 0..(MAX_TAIL / 1024 + 64) {
            writeln!(
                file,
                "{}",
                json!({"type": "user", "message": {"content": filler}})
            )
            .unwrap();
        }
        for index in 0..3 {
            writeln!(
                file,
                "{}",
                json!({"type": "assistant", "message": {"content": format!("m{index}")}})
            )
            .unwrap();
        }
        drop(file);
        let entries = read(&path, 2).unwrap();
        assert_eq!(
            entries,
            [
                Entry::Assistant {
                    text: "m1".into(),
                    at: None
                },
                Entry::Assistant {
                    text: "m2".into(),
                    at: None
                },
            ]
        );
        assert!(matches!(
            read(&dir.path().join("gone.jsonl"), 2),
            Err(Error::NoTranscript)
        ));
    }

    #[test]
    fn summarizes_known_and_unknown_tools() {
        assert_eq!(
            tool_summary("Edit", &json!({"file_path": "/a.rs", "old_string": "x"})),
            "/a.rs"
        );
        assert_eq!(
            tool_summary("mcp__db__query", &json!({"limit": 3, "sql": "select 1"})),
            "select 1"
        );
        assert_eq!(tool_summary("Bash", &json!({})), "");
        assert!(tool_summary("Bash", &json!({"command": "é".repeat(MAX_SUMMARY)})).ends_with('…'));
    }
}
