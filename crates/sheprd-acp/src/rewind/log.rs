//! Finding a prompt in Claude's session log. A log is data: only the fields
//! read here are looked at, and nothing in it is run.

use std::{
    collections::VecDeque,
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

use serde_json::Value;

use super::Error;
use crate::harness::out::Rewind;

/// Longer lines hold pictures, not a prompt to compare, and are skipped
/// after being read past.
const LINE: usize = 16 * 1024 * 1024;

/// A prompt the log holds.
struct Prompt {
    /// The last assistant message before it.
    anchor: Option<String>,
    text: String,
}

/// A record's prompt text, when it is a person's prompt: tool results also
/// come as `user` records, and a harness's own messages say so.
fn prompt_text(record: &Value) -> Option<String> {
    if record["isSidechain"] == true
        || record["isMeta"] == true
        || record["promptSource"] == "system"
    {
        return None;
    }
    let content = &record["message"]["content"];
    if let Some(text) = content.as_str() {
        return Some(text.to_owned());
    }
    content
        .as_array()?
        .iter()
        .find(|block| block["type"] == "text")
        .and_then(|block| block["text"].as_str())
        .map(str::to_owned)
}

/// The id of the assistant message the session ends at when it goes back to
/// before `rewind`'s prompt, or `None` when no message came before it.
pub(super) fn claude_anchor(path: &Path, rewind: &Rewind) -> Result<Option<String>, Error> {
    let file = File::open(path).map_err(Error::Log)?;
    anchor(BufReader::new(file), rewind)
}

fn anchor(mut reader: impl BufRead, rewind: &Rewind) -> Result<Option<String>, Error> {
    // Only the newest prompts of that text can be the one wanted.
    let mut matching: VecDeque<Prompt> = VecDeque::new();
    let mut last = None;
    let mut line = Vec::new();
    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line).map_err(Error::Log)? == 0 {
            break;
        }
        if line.len() > LINE {
            continue;
        }
        let Ok(record) = serde_json::from_slice::<Value>(&line) else {
            continue;
        };
        match record["type"].as_str() {
            Some("assistant") if record["isSidechain"] != true => {
                if let Some(uuid) = record["uuid"].as_str() {
                    last = Some(uuid.to_owned());
                }
            }
            Some("user") => {
                let Some(text) = prompt_text(&record).filter(|text| *text == rewind.text) else {
                    continue;
                };
                if matching.len() == rewind.occurrence {
                    matching.pop_front();
                }
                matching.push_back(Prompt {
                    anchor: last.clone(),
                    text,
                });
            }
            _ => {}
        }
    }
    let found = rewind
        .find(matching.iter().map(|prompt| prompt.text.as_str()))
        .ok_or(Error::NoPrompt)?;
    Ok(matching
        .swap_remove_back(found)
        .and_then(|prompt| prompt.anchor))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
