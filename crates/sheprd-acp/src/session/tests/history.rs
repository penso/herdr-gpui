//! A continued session opening with the history its agent's log holds.

use super::*;
use sheprd_chat_protocol::{Entry, EntryId, EntryKind, Transcript, Usage};

fn history() -> Transcript {
    let mut transcript = Transcript::new("Claude".to_owned());
    transcript.entries = vec![Entry {
        id: EntryId(4096),
        kind: EntryKind::Agent {
            text: "earlier".to_owned(),
        },
    }];
    transcript.truncated = true;
    transcript.usage = Some(Usage {
        cost: None,
        size: 200_000,
        used: 50_000,
    });
    transcript
}

#[test]
fn history_shows_before_the_session_opens_with_its_context_use() {
    let harness = harness_with(Config {
        history: Some(history()),
        ..config()
    });
    let transcript = harness.session.transcript();
    assert_eq!(transcript.entries, history().entries);
    assert!(transcript.truncated);
    assert_eq!(transcript.usage, history().usage);
}

#[test]
fn entries_after_the_history_come_after_it() {
    let mut harness = harness_with(Config {
        history: Some(history()),
        ..config()
    });
    harness
        .session
        .notice(NoticeLevel::Warning, "later".to_owned());
    let ids: Vec<_> = harness
        .session
        .transcript()
        .entries
        .iter()
        .map(|entry| entry.id)
        .collect();
    assert_eq!(ids.len(), 2);
    assert!(ids[0] < ids[1]);
}
