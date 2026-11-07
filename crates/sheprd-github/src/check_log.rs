//! The end of a GitHub Actions job's log, and the excerpt of it a prompt
//! carries. GitHub answers a log request with a redirect to storage that
//! needs no token, so the token is sent to GitHub alone. A failure is
//! written last, so the reader keeps a log's tail rather than its start,
//! whatever its size. Log text is untrusted.

use crate::http::{agent, authorization};
use secrecy::SecretString;
use std::{collections::VecDeque, io::Read, time::Duration};

mod error;
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

pub use error::Error;

/// Bytes of an excerpt at most.
pub const MAX_EXCERPT: usize = 8 * 1024;
/// Lines of an excerpt before the failure it ends on.
const LINES_BEFORE: usize = 80;
/// Bytes kept of the end of a log.
const TAIL: usize = 256 * 1024;
/// Bytes read of a log before giving up on reaching its end.
const READ_LIMIT: u64 = 64 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(20);
/// What GitHub Actions writes before a line that failed the step.
const ERROR_MARK: &str = "##[error]";

/// The end of job `job`'s log in `owner/repo`, which the caller took from a
/// verified pull request. Blocking.
pub fn job_log(token: &SecretString, owner: &str, repo: &str, job: u64) -> crate::Result<String> {
    let agent = agent(TIMEOUT);
    let context = "check log"; // i18n: data
    let reply = agent
        .get(format!(
            "https://api.github.com/repos/{owner}/{repo}/actions/jobs/{job}/logs"
        ))
        .header("User-Agent", "Sheprd")
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("Authorization", authorization(token)?)
        .call()
        .map_err(crate::log::network(context))?;
    let status = reply.status().as_u16();
    let location = reply
        .headers()
        .get("location")
        .and_then(|value| value.to_str().ok());
    let Some(location) = storage(status, location) else {
        return Err(refusal(status));
    };
    let mut log = agent
        .get(location)
        .header("User-Agent", "Sheprd")
        .call()
        .map_err(crate::log::network(context))?;
    let status = log.status().as_u16();
    if !(200..=299).contains(&status) {
        return Err(crate::Error::Status(status));
    }
    let tail = tail(log.body_mut().as_reader(), TAIL, READ_LIMIT).map_err(Error::Read)?;
    Ok(String::from_utf8_lossy(&tail).into_owned())
}

/// Where a log request's redirect leads. It carries no token, but a plain
/// address would still let the network rewrite the log.
fn storage(status: u16, location: Option<&str>) -> Option<&str> {
    location.filter(|url| (300..400).contains(&status) && url.starts_with("https://"))
}

fn refusal(status: u16) -> crate::Error {
    match status {
        401 => crate::Error::Authentication,
        403 => crate::Error::Forbidden,
        300..=399 => Error::Location.into(),
        status => crate::Error::Status(status),
    }
}

/// The last `keep` bytes of at most `limit` read from `reader`.
fn tail(reader: impl Read, keep: usize, limit: u64) -> std::io::Result<Vec<u8>> {
    let mut reader = reader.take(limit);
    let mut kept = VecDeque::with_capacity(keep);
    let mut chunk = [0; 16 * 1024];
    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            return Ok(kept.into());
        }
        kept.extend(&chunk[..read]);
        kept.drain(..kept.len().saturating_sub(keep));
    }
}

/// What a prompt quotes of `log`: the lines leading to its last error, or
/// its end when it marks none, as plain text within [`MAX_EXCERPT`].
pub fn excerpt(log: &str) -> String {
    let lines: Vec<String> = log.lines().map(plain).collect();
    let end = lines
        .iter()
        .rposition(|line| line.contains(ERROR_MARK))
        .map_or(lines.len(), |line| line + 1);
    let start = end.saturating_sub(LINES_BEFORE + 1);
    let text = lines[start..end].join("\n");
    let mut from = text.len().saturating_sub(MAX_EXCERPT);
    while !text.is_char_boundary(from) {
        from += 1;
    }
    let kept = &text[from..];
    // A cut line reads as a mistake; the next whole one starts better.
    let kept = match kept.split_once('\n') {
        Some((_, rest)) if from > 0 => rest,
        _ => kept,
    };
    kept.trim().to_owned()
}

/// A log line without its timestamp, colors, or anything else that is not
/// text.
fn plain(line: &str) -> String {
    let line = line.strip_prefix('\u{feff}').unwrap_or(line);
    let line = match line.split_once(' ') {
        Some((stamp, rest)) if timestamp(stamp) => rest,
        _ => line,
    };
    let mut text = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // A color: `ESC [`, parameters, then one final letter.
            if chars.next() == Some('[') {
                chars.find(|c| ('\u{40}'..='\u{7e}').contains(c));
            }
        } else if c == '\t' {
            text.push_str("    ");
        } else if !c.is_control() && !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        {
            text.push(c);
        }
    }
    text
}

/// Whether `word` is the time GitHub Actions starts each line with.
fn timestamp(word: &str) -> bool {
    word.len() >= 20 && word.ends_with('Z') && word.as_bytes()[10] == b'T'
}
