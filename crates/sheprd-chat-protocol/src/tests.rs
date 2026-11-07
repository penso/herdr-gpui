#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

mod activities;
mod agents;
mod assist;
mod auto_accept;
mod change;
mod display;
mod endpoint;
mod frame;
mod media;
mod notices;
mod revisions;
mod wire;

fn text(id: u64, kind: fn(String) -> EntryKind, value: &str) -> Entry {
    Entry {
        id: EntryId(id),
        kind: kind(value.to_owned()),
    }
}

fn agent(id: u64, value: &str) -> Entry {
    text(id, |text| EntryKind::Agent { text }, value)
}

fn user(id: u64, value: &str) -> Entry {
    text(id, |text| EntryKind::User { text, files: 0 }, value)
}

fn tool(id: u64, status: ToolStatus) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::Tool(Tool {
            call_id: "call-1".to_owned(),
            content: Vec::new(),
            kind: ToolKind::Execute,
            locations: Vec::new(),
            parent: None,
            started_ms: None,
            status,
            title: "cargo test".to_owned(),
        }),
    }
}

fn permission(id: u64) -> Permission {
    Permission {
        call_id: Some("call-1".to_owned()),
        id: PermissionId(id),
        options: vec![PermissionOption {
            id: "allow".to_owned(),
            kind: PermissionKind::AllowOnce,
            name: "Allow".to_owned(),
        }],
        title: "Run cargo test?".to_owned(),
    }
}

#[test]
fn a_prompts_files_are_kept_until_its_entry_is_dropped() {
    let mut transcript = Transcript::new("Claude");
    let files = |id| PromptFiles {
        attachments: vec![Attachment {
            path: "/work/notes.md".to_owned(),
        }],
        id: EntryId(id),
    };
    for id in [1, 5] {
        let files = files(id);
        transcript.apply(&Change::PromptFiles { files }).unwrap();
    }
    transcript
        .apply(&Change::Truncate { before: EntryId(3) })
        .unwrap();
    assert_eq!(transcript.prompt_files, [files(5)]);
}

#[test]
fn dropping_entries_counts_the_prompts_among_them() {
    let entry = |id, kind| Entry {
        id: EntryId(id),
        kind,
    };
    let prompt = || EntryKind::User {
        text: "go".into(),
        files: 0,
    };
    let mut transcript = Transcript::new("Claude");
    transcript.dropped_prompts = 2;
    transcript.entries = vec![
        entry(1, prompt()),
        entry(2, EntryKind::Agent { text: "ok".into() }),
        entry(3, prompt()),
    ];
    let before = EntryId(3);
    transcript.apply(&Change::Truncate { before }).unwrap();
    assert_eq!(transcript.dropped_prompts, 3);
    transcript.apply(&Change::Truncate { before }).unwrap();
    assert_eq!(transcript.dropped_prompts, 3, "nothing more went");
}

#[test]
fn a_transcript_from_before_rewinds_reads_as_unable() {
    let session: SessionInfo = serde_json::from_str(r#"{"id": "s"}"#).unwrap();
    assert!(!session.rewind);
}
