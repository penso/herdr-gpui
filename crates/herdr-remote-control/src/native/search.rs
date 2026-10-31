//! `GET /api/v2/search?q=`: what was said in this computer's chats. The
//! phone searches every chat the daemon follows, not only the ones it has
//! opened and cached.

use crate::web::{ApiError, Device, Shared};
use axum::{
    Json,
    extract::{Query, State},
};
use herdr_chat_protocol::{Author, EntryId, EntryKind, Transcript};
use serde::{Deserialize, Serialize};

/// The fewest characters a query needs, so one letter does not match
/// every chat.
pub(crate) const MIN_QUERY: usize = 2;
/// The most characters a query may have.
pub(crate) const MAX_QUERY: usize = 100;
/// Matches kept per chat, newest first.
pub(crate) const HITS_PER_CHAT: usize = 3;
/// Matches in one answer.
pub(crate) const MAX_HITS: usize = 60;
/// Characters of context kept before a match in its snippet.
const BEFORE: usize = 40;
/// The longest snippet, in characters.
const SNIPPET: usize = 160;

#[derive(Deserialize)]
pub(crate) struct SearchQuery {
    q: String,
}

#[derive(Debug, Default, Eq, PartialEq, Serialize)]
pub(crate) struct SearchResults {
    pub(crate) hits: Vec<Hit>,
    /// More matched than [`MAX_HITS`] or [`HITS_PER_CHAT`] let through.
    pub(crate) truncated: bool,
}

/// One message that matched: its chat, which entry, who said it, and the
/// words around the match on one line.
#[derive(Debug, Eq, PartialEq, Serialize)]
pub(crate) struct Hit {
    pub(crate) pane: String,
    pub(crate) entry: EntryId,
    pub(crate) author: Author,
    pub(crate) snippet: String,
}

/// `GET /api/v2/search?q=`.
pub(crate) async fn search(
    State(state): State<Shared>,
    _device: Device,
    Query(query): Query<SearchQuery>,
) -> Result<Json<SearchResults>, ApiError> {
    let needle = Needle::parse(&query.q)?;
    let layout = state.hub.layout();
    let mut results = SearchResults::default();
    for chat in layout.chats() {
        let Some(link) = state.hub.chat(&chat.pane) else {
            continue;
        };
        let room = MAX_HITS - results.hits.len();
        if room == 0 {
            results.truncated = true;
            break;
        }
        let (hits, more) = link.read(|transcript| matches(transcript, &needle, room));
        results.truncated |= more;
        results
            .hits
            .extend(hits.into_iter().map(|(entry, author, snippet)| Hit {
                pane: chat.pane.clone(),
                entry,
                author,
                snippet,
            }));
    }
    Ok(Json(results))
}

/// A query folded for case-insensitive matching.
pub(crate) struct Needle(Vec<char>);

impl Needle {
    pub(crate) fn parse(query: &str) -> Result<Self, ApiError> {
        let query = query.split_whitespace().collect::<Vec<_>>().join(" ");
        let length = query.chars().count();
        if length < MIN_QUERY {
            return Err(ApiError::Invalid("that search is too short"));
        }
        if length > MAX_QUERY {
            return Err(ApiError::Invalid("that search is too long"));
        }
        Ok(Self(query.chars().flat_map(char::to_lowercase).collect()))
    }
}

/// The newest user and agent messages in `transcript` that contain
/// `needle`, at most `room` of them and [`HITS_PER_CHAT`]; and whether
/// more matched.
pub(crate) fn matches(
    transcript: &Transcript,
    needle: &Needle,
    room: usize,
) -> (Vec<(EntryId, Author, String)>, bool) {
    let limit = room.min(HITS_PER_CHAT);
    let mut hits = Vec::new();
    for entry in transcript.entries.iter().rev() {
        let (author, text) = match &entry.kind {
            EntryKind::User { text, .. } => (Author::User, text),
            EntryKind::Agent { text } => (Author::Agent, text),
            _ => continue,
        };
        let Some(at) = find(text, needle) else {
            continue;
        };
        if hits.len() == limit {
            return (hits, true);
        }
        hits.push((entry.id, author, snippet(text, at)));
    }
    (hits, false)
}

/// The byte offset in `text` where `needle` starts, ignoring case and
/// treating any run of whitespace as one space.
pub(crate) fn find(text: &str, needle: &Needle) -> Option<usize> {
    let mut folded: Vec<(char, usize)> = Vec::with_capacity(text.len());
    let mut space = false;
    for (at, character) in text.char_indices() {
        if character.is_whitespace() {
            if !space && !folded.is_empty() {
                folded.push((' ', at));
            }
            space = true;
            continue;
        }
        space = false;
        folded.extend(character.to_lowercase().map(|lower| (lower, at)));
    }
    let needle = &needle.0;
    folded
        .windows(needle.len())
        .find(|window| {
            window
                .iter()
                .map(|(character, _)| character)
                .eq(needle.iter())
        })
        .map(|window| window[0].1)
}

/// The words around byte `at` of `text` on one line, cut at
/// [`SNIPPET`] characters with an ellipsis where text was left out.
pub(crate) fn snippet(text: &str, at: usize) -> String {
    let start = text[..at]
        .char_indices()
        .rev()
        .nth(BEFORE.saturating_sub(1))
        .map_or(0, |(start, _)| start);
    let line = text[start..]
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let mut snippet = String::new();
    if start > 0 {
        snippet.push('…');
    }
    let room = SNIPPET - snippet.chars().count();
    if line.chars().count() > room {
        snippet.extend(line.chars().take(room - 1));
        snippet.push('…');
    } else {
        snippet.push_str(&line);
    }
    snippet
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
