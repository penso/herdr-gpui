//! When a chat change is worth a push, and what it says. Pure: the daemon
//! passes each change with the transcript it applies to, before applying it.

use super::{Message, Urgency};
use herdr_chat_protocol::{
    Change, EntryKind, NoticeLevel, Status, Transcript, TurnEnd, TurnSummary, display,
};
use serde::{Deserialize, Serialize};

/// The most characters of chat text an excerpt shows.
pub(super) const EXCERPT_CHARS: usize = 120;

/// Which chat a change belongs to, as the phone names it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Chat<'a> {
    /// The agent's display name, such as `Claude`.
    pub agent: &'a str,
    pub pane: &'a str,
    pub title: &'a str,
    /// The workspace, such as `herdr-gpui`.
    pub workspace: &'a str,
}

/// How much of the conversation a push may carry. Push services relay only
/// ciphertext, but a notification shows on a locked screen.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Privacy {
    /// Who asked and where, never what was said.
    #[default]
    Generic,
    /// Also a short excerpt: the question, the error, or the answer's end.
    Excerpt,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Kind {
    // keep-sorted start
    Error,
    Question,
    TurnEnded,
    // keep-sorted end
}

/// A push to send for one chat.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Notification {
    pub body: String,
    /// Closes the chat's notification instead of showing one: its last
    /// question was answered, from whichever client.
    pub clear: bool,
    pub kind: Kind,
    pub pane: String,
    /// One per chat, so a newer push replaces the older one.
    pub tag: String,
    pub title: String,
    pub urgency: Urgency,
}

impl Notification {
    fn new(chat: &Chat<'_>, kind: Kind, title: String, body: String) -> Self {
        Self {
            body,
            clear: false,
            kind,
            pane: chat.pane.to_owned(),
            tag: tag(chat.pane),
            title,
            urgency: if kind == Kind::Question {
                Urgency::High
            } else {
                Urgency::Normal
            },
        }
    }

    fn clear(chat: &Chat<'_>) -> Self {
        Self {
            clear: true,
            ..Self::new(chat, Kind::Question, String::new(), String::new())
        }
    }

    /// The payload the service worker receives, with `badge` questions
    /// waiting across every chat.
    pub(crate) fn message(&self, badge: u32) -> Message {
        Message {
            badge,
            body: self.body.clone(),
            clear: self.clear,
            kind: self.kind,
            tag: self.tag.clone(),
            title: self.title.clone(),
            url: format!("/chat/{}", path_segment(&self.pane)),
        }
    }
}

/// `text` percent-encoded as one URL path segment: only unreserved
/// characters stay as they are.
fn path_segment(text: &str) -> String {
    let mut segment = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            segment.push(char::from(byte));
        } else {
            segment.push_str(&format!("%{byte:02X}"));
        }
    }
    segment
}

/// The notification tag for the chat in `pane`.
pub(crate) fn tag(pane: &str) -> String {
    format!("chat-{pane}")
}

/// The push `change` calls for, given the transcript `before` it applies.
pub(crate) fn decide(
    chat: &Chat<'_>,
    before: &Transcript,
    change: &Change,
    privacy: Privacy,
) -> Option<Notification> {
    let excerpt = |text: &str| (privacy == Privacy::Excerpt).then(|| excerpt(text));
    match change {
        Change::PermissionAsked { permission } => {
            // Answered by itself at once; nobody needs to look.
            if before.auto_accept {
                return None;
            }
            let title = format!("{} needs an answer", chat.agent);
            Some(Notification::new(
                chat,
                Kind::Question,
                title,
                body(chat, excerpt(&permission.title), None),
            ))
        }
        Change::PermissionResolved { id } => {
            let last = matches!(before.permissions.as_slice(), [only] if only.id == *id);
            last.then(|| Notification::clear(chat))
        }
        Change::Push { entry } => match &entry.kind {
            EntryKind::Turn(summary) => turn_ended(chat, before, summary, privacy),
            EntryKind::Notice {
                level: NoticeLevel::Error,
                text,
                ..
            } => {
                let title = format!("{} hit an error", chat.agent);
                Some(Notification::new(
                    chat,
                    Kind::Error,
                    title,
                    body(chat, excerpt(text), None),
                ))
            }
            _ => None,
        },
        Change::Status { status } if *status != before.status => {
            let (title, message) = match status {
                Status::Failed { message } => (format!("{} failed", chat.agent), message),
                // An agent that quits while it waits on a question leaves it
                // unanswerable; any other exit is the user's doing.
                Status::Exited { message } if before.is_blocked() => {
                    (format!("{} stopped while asking", chat.agent), message)
                }
                _ => return None,
            };
            Some(Notification::new(
                chat,
                Kind::Error,
                title,
                body(chat, excerpt(message), None),
            ))
        }
        _ => None,
    }
}

fn turn_ended(
    chat: &Chat<'_>,
    before: &Transcript,
    summary: &TurnSummary,
    privacy: Privacy,
) -> Option<Notification> {
    // The user stopped it, so they know.
    if summary.end == TurnEnd::Cancelled {
        return None;
    }
    let mut facts = summary.describe();
    if let Some(cost) = before.usage.as_ref().and_then(|usage| usage.cost.as_ref()) {
        facts.push_str(" \u{b7} ");
        facts.push_str(&display::cost(cost));
    }
    let answer = (privacy == Privacy::Excerpt)
        .then(|| last_answer(before))
        .flatten()
        .map(excerpt);
    let title = format!("{} finished", chat.agent);
    Some(Notification::new(
        chat,
        Kind::TurnEnded,
        title,
        body(chat, answer, Some(&facts)),
    ))
}

/// The text of the agent's last answer in the transcript.
fn last_answer(transcript: &Transcript) -> Option<&str> {
    transcript
        .entries
        .iter()
        .rev()
        .find_map(|entry| match &entry.kind {
            EntryKind::Agent { text } if !text.trim().is_empty() => Some(text.as_str()),
            _ => None,
        })
}

/// `excerpt` (when allowed) above `workspace · title`, then any `facts`.
fn body(chat: &Chat<'_>, excerpt: Option<String>, facts: Option<&str>) -> String {
    let place = if chat.title.is_empty() {
        chat.workspace.to_owned()
    } else {
        format!("{} \u{b7} {}", chat.workspace, chat.title)
    };
    let lines = [excerpt.as_deref(), Some(place.as_str()), facts];
    lines.into_iter().flatten().collect::<Vec<_>>().join("\n")
}

/// `text` on one line, at most [`EXCERPT_CHARS`] characters, ending in `…`
/// when cut.
pub(crate) fn excerpt(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    let line = words.join(" ");
    if line.chars().count() <= EXCERPT_CHARS {
        return line;
    }
    let mut cut: String = line.chars().take(EXCERPT_CHARS - 1).collect();
    cut.truncate(cut.trim_end().len());
    cut.push('\u{2026}');
    cut
}
