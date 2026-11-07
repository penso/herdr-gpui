//! Tokens an agent used, read from its own session files. Each harness
//! keeps them differently; one this module does not know reports nothing,
//! which reads as unknown, never as zero. The files are the agent's and
//! untrusted: reads are bounded and malformed lines are skipped.

use crate::herdr::Session;
use serde_json::Value;
use sheprd_automation::report::Tokens;
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

/// The most of one session file read.
const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;
/// The most files looked at for one agent.
const MAX_FILES: usize = 64;

/// The tokens an agent of `harness` working in `cwd` since `since` used.
pub fn tokens(
    harness: &str,
    cwd: &str,
    since: i64,
    session: Option<&Session>,
    home: &Path,
) -> Option<Tokens> {
    match harness {
        "claude" => claude(cwd, since, session, home),
        "codex" => codex(cwd, since, session, home),
        _ => None,
    }
}

fn claude(cwd: &str, since: i64, session: Option<&Session>, home: &Path) -> Option<Tokens> {
    let root = std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".claude"))
        .join("projects");
    let dir = root.join(claude_project_dir(cwd));
    let files = match session {
        Some(session) if session.kind == "path" => vec![PathBuf::from(&session.value)],
        Some(session) => vec![dir.join(format!("{}.jsonl", session.value))],
        None => recent_files(&dir, since, |name| name.ends_with(".jsonl")),
    };
    let mut seen = HashSet::new();
    let mut total = None::<Tokens>;
    for file in files.iter().take(MAX_FILES) {
        for line in lines(file) {
            let Some(found) = claude_line(&line, &mut seen) else {
                continue;
            };
            total = Some(total.unwrap_or_default() + found);
        }
    }
    total
}

/// One transcript line's usage, once per message: Claude repeats a
/// message's usage on every line of that message.
pub(crate) fn claude_line(line: &str, seen: &mut HashSet<String>) -> Option<Tokens> {
    let value = serde_json::from_str::<Value>(line).ok()?;
    if value.get("type").and_then(Value::as_str) != Some("assistant") {
        return None;
    }
    let message = value.get("message")?;
    let usage = message.get("usage")?;
    let id = message
        .get("id")
        .and_then(Value::as_str)
        .or_else(|| value.get("uuid").and_then(Value::as_str))?;
    if !seen.insert(id.to_owned()) {
        return None;
    }
    let count = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
    Some(Tokens {
        input: count("input_tokens"),
        output: count("output_tokens"),
        cache_read: count("cache_read_input_tokens"),
        cache_write: count("cache_creation_input_tokens"),
    })
}

fn codex(cwd: &str, since: i64, session: Option<&Session>, home: &Path) -> Option<Tokens> {
    let root = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".codex"))
        .join("sessions");
    let mut files = match session {
        Some(session) if session.kind == "path" => vec![PathBuf::from(&session.value)],
        _ => {
            // Rollouts live under sessions/YYYY/MM/DD; walk only recent days.
            let mut found = Vec::new();
            for day in days_since(since) {
                let dir = root.join(day);
                let id = session.map(|session| session.value.clone());
                found.extend(recent_files(&dir, since, |name| {
                    name.starts_with("rollout-")
                        && name.ends_with(".jsonl")
                        && id.as_ref().is_none_or(|id| name.contains(id.as_str()))
                }));
            }
            found
        }
    };
    files.truncate(MAX_FILES);
    let mut total = None::<Tokens>;
    for file in &files {
        let mut cwd_matches = session.is_some();
        let mut last = None;
        for line in lines(file) {
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            match value.get("type").and_then(Value::as_str) {
                Some("session_meta") => {
                    cwd_matches |=
                        value.pointer("/payload/cwd").and_then(Value::as_str) == Some(cwd);
                }
                Some("event_msg") => {
                    if let Some(found) = codex_line(&value) {
                        last = Some(found);
                    }
                }
                _ => {}
            }
        }
        if cwd_matches && let Some(found) = last {
            total = Some(total.unwrap_or_default() + found);
        }
    }
    total
}

/// A rollout's running total, from a `token_count` event. Codex counts
/// cached input inside `input_tokens`.
pub(crate) fn codex_line(value: &Value) -> Option<Tokens> {
    let payload = value.get("payload")?;
    if payload.get("type").and_then(Value::as_str) != Some("token_count") {
        return None;
    }
    let usage = payload.pointer("/info/total_token_usage")?;
    let count = |key: &str| usage.get(key).and_then(Value::as_u64).unwrap_or(0);
    let cached = count("cached_input_tokens");
    Some(Tokens {
        input: count("input_tokens").saturating_sub(cached),
        output: count("output_tokens"),
        cache_read: cached,
        cache_write: 0,
    })
}

/// Files in `dir` modified at or after `since` whose name passes `keep`.
fn recent_files(dir: &Path, since: i64, keep: impl Fn(&str) -> bool) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files = entries
        .flatten()
        .filter(|entry| keep(&entry.file_name().to_string_lossy()))
        .filter(|entry| {
            entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                .and_then(|age| i64::try_from(age.as_secs()).ok())
                .is_some_and(|modified| modified >= since)
        })
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    files.sort();
    files
}

/// `YYYY/MM/DD` for every day from `since` to today, in UTC, at most 8.
fn days_since(since: i64) -> Vec<String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|now| i64::try_from(now.as_secs()).ok())
        .unwrap_or(since);
    let mut days = Vec::new();
    let mut at = since.min(now);
    while at <= now + 86_400 && days.len() < 8 {
        if let Ok(day) = jiff::Timestamp::from_second(at) {
            let date = day.to_zoned(jiff::tz::TimeZone::UTC).date();
            let text = format!("{:04}/{:02}/{:02}", date.year(), date.month(), date.day());
            if !days.contains(&text) {
                days.push(text);
            }
        }
        at += 86_400;
    }
    days
}

/// The lines of a file, at most [`MAX_FILE_BYTES`] of it.
fn lines(path: &Path) -> impl Iterator<Item = String> {
    File::open(path)
        .ok()
        .map(|file| {
            BufReader::new(file.take(MAX_FILE_BYTES))
                .lines()
                .map_while(Result::ok)
        })
        .into_iter()
        .flatten()
}

/// Claude's project directory for a working directory: every character
/// that is not ASCII alphanumeric becomes `-`; past 200 characters the name
/// is cut and a Java-style hash of the full path, in base 36, appended. As
/// in Sheprd's teleport.
pub(crate) fn claude_project_dir(cwd: &str) -> String {
    const LIMIT: usize = 200;
    let name = cwd
        .encode_utf16()
        .map(|unit| match char::from_u32(u32::from(unit)) {
            Some(c) if c.is_ascii_alphanumeric() => c,
            _ => '-',
        })
        .collect::<String>();
    if name.len() <= LIMIT {
        return name;
    }
    let hash = cwd.encode_utf16().fold(0i32, |hash, unit| {
        hash.wrapping_shl(5)
            .wrapping_sub(hash)
            .wrapping_add(i32::from(unit))
    });
    format!(
        "{}-{}",
        &name[..LIMIT],
        base36(i64::from(hash).unsigned_abs())
    )
}

fn base36(mut value: u64) -> String {
    const DIGITS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".to_owned();
    }
    let mut digits = Vec::new();
    while value > 0 {
        digits.push(
            DIGITS
                .get(usize::try_from(value % 36).unwrap_or(0))
                .copied()
                .unwrap_or(b'0'),
        );
        value /= 36;
    }
    digits.reverse();
    String::from_utf8(digits).unwrap_or_default()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
