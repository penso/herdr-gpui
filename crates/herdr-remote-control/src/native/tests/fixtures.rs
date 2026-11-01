//! The JSON the native app decodes, checked byte for byte against the
//! fixtures its Swift tests read. `HERDR_UPDATE_FIXTURES=1` rewrites them.

use super::*;
use crate::{
    control::pull_requests::{PullRequests, Tone, WorkspacePullRequest},
    hub::Summary,
    load::Load,
};
use herdr_chat_protocol::{Cursor, Entry, EntryId, EntryKind, Status, Transcript, Turn, Usage};
use herdr_client::protocol::ClientShellWorktree;
use model::{ChatSnapshot, Home, Sources};
use serde::Serialize;
use std::path::PathBuf;
use wire::{ByeReason, Step, TermSize, View};

/// Where the Swift package keeps its v2 fixtures.
pub(super) fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../ios/HerdrRemoteCore/Tests/HerdrRemoteCoreTests/Fixtures/v2")
}

/// Compares `value` as pretty JSON with the fixture `name`, or writes it
/// when `HERDR_UPDATE_FIXTURES` is set.
pub(super) fn check(name: &str, value: &impl Serialize) {
    let path = dir().join(format!("{name}.json"));
    let json = format!("{}\n", serde_json::to_string_pretty(value).unwrap());
    if std::env::var_os("HERDR_UPDATE_FIXTURES").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, json).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    assert_eq!(
        json,
        expected,
        "{} is out of date; run with HERDR_UPDATE_FIXTURES=1",
        path.display()
    );
}

pub(super) fn cursor(seq: u64) -> Cursor {
    Cursor {
        epoch: uuid::Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef),
        seq,
    }
}

fn transcript() -> Transcript {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Working;
    transcript.turn = Some(Turn {
        started_ms: 1_761_670_000_000,
    });
    transcript.usage = Some(Usage {
        cost: None,
        size: 200_000,
        used: 84_000,
    });
    transcript.entries = vec![
        Entry {
            id: EntryId(1),
            kind: EntryKind::User {
                text: "Make it even out nested splits too.".into(),
                files: 0,
            },
        },
        Entry {
            id: EntryId(2),
            kind: EntryKind::Agent {
                text: "Added **Equalize Splits**.".into(),
            },
        },
    ];
    transcript.permissions = vec![permission(3)];
    transcript
}

fn home() -> Home {
    let mut layout = layout(EndpointId::random());
    layout.workspaces.push(Workspace {
        id: "w2".into(),
        label: "moltis".into(),
        branch: None,
        cwd: "/moltis".into(),
        worktree: None,
        chats: Vec::new(),
        terminals: Vec::new(),
    });
    let worktree = |linked| ClientShellWorktree {
        key: "/work/.git".into(),
        label: "work".into(),
        is_linked_worktree: linked,
    };
    layout.workspaces[0].worktree = Some(worktree(false));
    layout.workspaces.push(Workspace {
        id: "w3".into(),
        label: "spaces tab".into(),
        branch: Some("worktree/spaces-tab".into()),
        cwd: "/work-spaces-tab".into(),
        worktree: Some(worktree(true)),
        chats: Vec::new(),
        terminals: Vec::new(),
    });
    layout.workspaces[0].chats.push(ChatRow {
        pane: "p3".into(),
        tab: "t3".into(),
        endpoint: EndpointId::random(),
        agent: None,
        title: "Starting".into(),
        status: AgentStatus::Unknown,
    });
    let summary = Summary {
        connected: true,
        status: Status::Working,
        question: Some(permission(3)),
        questions: 1,
        last_text: Some("Added Equalize Splits.".into()),
        turn_started_ms: Some(1_761_670_000_000),
        last_turn_ended_ms: None,
        agent: "Claude".into(),
        usage_percent: Some(42),
    };
    let pull_requests = PullRequests {
        boot_id: "b".into(),
        entries: vec![WorkspacePullRequest {
            workspace_id: "w1".into(),
            cwd: "/work".into(),
            branch: "main".into(),
            number: 870,
            url: "https://github.com/penso/herdr-gpui/pull/870".into(),
            tone: Tone::Waiting,
        }],
    };
    Home::project(Sources {
        computer: "MacBook Pro",
        fingerprint: &"AB".repeat(32),
        layout: &layout,
        load: Some(Load {
            cpu: 47,
            memory: 67,
        }),
        summaries: [("p1".to_owned(), summary)].into(),
        pull_requests: &pull_requests,
    })
}

fn snapshot() -> ChatSnapshot {
    ChatSnapshot {
        pane: "p1".into(),
        cursor: Some(cursor(41)),
        connected: true,
        transcript: transcript(),
    }
}

const SCREEN: &str = "\x1b[36m~/code\x1b[0m \x1b[1;32m❯\x1b[0m just ci\n\
    \x1b[1;31mFAIL\x1b[0m \x1b[2m[0.144s]\x1b[0m keymap\n\
    \x1b[38;2;255;128;0mrgb\x1b[0m \x1b[7minverse\x1b[0m \x1b[3;4mit\x1b[0m \x1b[94mbright\x1b[0m";

#[test]
fn server_messages_match_the_swift_fixtures() {
    check("home", &ServerMessage::Home { home: home() });
    check("chat", &snapshot());
    check(
        "chat_snapshot",
        &ServerMessage::ChatSnapshot(Box::new(snapshot())),
    );
    check(
        "chat_resumed",
        &ServerMessage::ChatResumed {
            pane: "p1".into(),
            cursor: cursor(41),
            connected: true,
        },
    );
    check(
        "chat_changes",
        &ServerMessage::ChatChanges {
            pane: "p1".into(),
            changes: vec![
                Step {
                    cursor: cursor(42),
                    change: herdr_chat_protocol::Change::Append {
                        id: EntryId(2),
                        text: " Tests cover nested splits.".into(),
                    },
                },
                Step {
                    cursor: cursor(43),
                    change: herdr_chat_protocol::Change::PermissionResolved {
                        id: PermissionId(3),
                    },
                },
                Step {
                    cursor: cursor(44),
                    change: herdr_chat_protocol::Change::Status {
                        status: Status::Idle,
                    },
                },
            ],
        },
    );
    check(
        "chat_connection",
        &ServerMessage::ChatConnection {
            pane: "p1".into(),
            connected: false,
        },
    );
    let (columns, rows) = model::screen(SCREEN);
    check(
        "term_screen",
        &ServerMessage::TermScreen {
            pane: "p2".into(),
            columns,
            rows,
        },
    );
    check(
        "reload",
        &ServerMessage::Reload {
            view: View::Chat { pane: "p1".into() },
        },
    );
    check(
        "error",
        &ServerMessage::Error {
            message: "no such chat".into(),
        },
    );
    check(
        "bye",
        &ServerMessage::Bye {
            reason: ByeReason::Revoked,
        },
    );
}

#[test]
fn client_messages_match_the_swift_fixtures() {
    check(
        "subscribe_home",
        &ClientMessage::Subscribe {
            view: View::Home,
            since: None,
            size: None,
        },
    );
    check(
        "subscribe_chat",
        &ClientMessage::Subscribe {
            view: View::Chat { pane: "p1".into() },
            since: Some(cursor(41)),
            size: None,
        },
    );
    check(
        "subscribe_term",
        &ClientMessage::Subscribe {
            view: View::Term { pane: "p2".into() },
            since: None,
            size: None,
        },
    );
    check(
        "subscribe_term_sized",
        &ClientMessage::Subscribe {
            view: View::Term { pane: "p2".into() },
            since: None,
            size: Some(TermSize { cols: 46, rows: 30 }),
        },
    );
    check(
        "unsubscribe_chat",
        &ClientMessage::Unsubscribe {
            view: View::Chat { pane: "p1".into() },
        },
    );
    check(
        "term_scroll",
        &ClientMessage::TermScroll {
            pane: "p2".into(),
            lines: 3,
        },
    );
}

#[test]
fn every_fixture_reads_back_as_what_wrote_it() {
    // The other tests are writing them now.
    if std::env::var_os("HERDR_UPDATE_FIXTURES").is_some() {
        return;
    }
    for name in [
        "home",
        "chat_snapshot",
        "chat_resumed",
        "chat_changes",
        "chat_connection",
        "term_screen",
        "reload",
        "error",
        "bye",
    ] {
        let text = std::fs::read_to_string(dir().join(format!("{name}.json"))).unwrap();
        let message: ServerMessage = serde_json::from_str(&text).unwrap();
        let again = format!("{}\n", serde_json::to_string_pretty(&message).unwrap());
        assert_eq!(again, text, "{name}");
    }
    for name in [
        "subscribe_home",
        "subscribe_chat",
        "subscribe_term",
        "subscribe_term_sized",
        "term_scroll",
        "unsubscribe_chat",
    ] {
        let text = std::fs::read_to_string(dir().join(format!("{name}.json"))).unwrap();
        let _: ClientMessage = serde_json::from_str(&text).unwrap();
    }
}
