#![allow(clippy::unwrap_used, clippy::expect_used)]

use herdr_chat_protocol::{
    Cursor, Entry, EntryId, EntryKind, Permission, PermissionId, PermissionOption, Queued,
    ServerMessage, Transcript,
};
use ratatui::{Terminal, backend::TestBackend};

use super::*;

fn screen(view: &mut View, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| draw(frame, view, 0)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol().to_owned())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn busy() -> View {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Working;
    transcript.clients = 1;
    transcript.entries = vec![
        Entry {
            id: EntryId(1),
            kind: EntryKind::User {
                text: "please fix the failing test in the parser module".to_owned(),
                files: 0,
            },
        },
        Entry {
            id: EntryId(2),
            kind: EntryKind::Agent {
                text: "Looking at it now.".to_owned(),
            },
        },
    ];
    transcript.permissions = vec![Permission {
        call_id: None,
        id: PermissionId(1),
        options: vec![PermissionOption {
            id: "allow".to_owned(),
            kind: PermissionKind::AllowOnce,
            name: "Allow once".to_owned(),
        }],
        title: "Run cargo test -p parser".to_owned(),
    }];
    transcript.queued = vec![Queued {
        attachments: Vec::new(),
        id: herdr_chat_protocol::CommandId::random(),
        text: "and then commit".to_owned(),
    }];
    let mut view = View::default();
    view.receive(ServerMessage::Snapshot {
        cursor: Cursor {
            epoch: uuid::Uuid::nil(),
            seq: 0,
        },
        transcript: Box::new(transcript),
    });
    view
}

#[test]
fn a_busy_session_shows_everything_it_needs() {
    let screen = screen(&mut busy(), 80, 20);
    for expected in [
        "Claude",
        "waiting for you",
        "1 client attached",
        "› please fix",
        "Looking at it now.",
        "↳ queued: and then commit",
        "Permission",
        "1. Allow once",
    ] {
        assert!(
            screen.contains(expected),
            "{expected} missing from\n{screen}"
        );
    }
}

#[test]
fn narrow_and_short_terminals_still_draw() {
    for (width, height) in [(20, 6), (12, 4), (1, 1), (200, 3)] {
        screen(&mut busy(), width, height);
    }
    let narrow = screen(&mut busy(), 30, 24);
    assert!(narrow.contains("Enter send · Ctrl+D quit"), "{narrow}");
}

#[test]
fn the_view_scrolls_back_and_clamps() {
    let mut view = busy();
    view.scroll = 1000;
    let screen = screen(&mut view, 80, 20);
    assert!(screen.contains("› please fix"));
    assert!(view.scroll < 1000);
}
