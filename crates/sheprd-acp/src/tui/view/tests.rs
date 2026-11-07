#![allow(clippy::unwrap_used, clippy::expect_used)]

use crossterm::event::KeyEventState;
use sheprd_chat_protocol::{
    Entry, EntryKind, Mode, Modes, Permission, PermissionId, PermissionKind, PermissionOption,
};
use uuid::Uuid;

use super::*;

mod attachments;

fn key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent {
        code,
        kind: KeyEventKind::Press,
        modifiers,
        state: KeyEventState::NONE,
    }
}

fn press(view: &mut View, code: KeyCode) -> Option<Out> {
    view.key(key(code, KeyModifiers::NONE))
}

fn ctrl(view: &mut View, ch: char) -> Option<Out> {
    view.key(key(KeyCode::Char(ch), KeyModifiers::CONTROL))
}

fn typed(view: &mut View, text: &str) {
    for ch in text.chars() {
        press(view, KeyCode::Char(ch));
    }
}

fn snapshot(view: &mut View, transcript: Transcript) -> Cursor {
    let cursor = Cursor {
        epoch: Uuid::new_v4(),
        seq: 1,
    };
    view.receive(ServerMessage::Snapshot {
        cursor,
        transcript: Box::new(transcript),
    });
    cursor
}

fn asking() -> Transcript {
    Transcript {
        permissions: vec![Permission {
            call_id: None,
            id: PermissionId(3),
            options: ["allow", "deny"]
                .into_iter()
                .map(|id| PermissionOption {
                    id: id.to_owned(),
                    kind: PermissionKind::Other,
                    name: id.to_owned(),
                })
                .collect(),
            title: "Run?".to_owned(),
        }],
        status: Status::Working,
        ..Transcript::new("Claude")
    }
}

#[test]
fn enter_sends_the_prompt_and_alt_enter_adds_a_line() {
    let mut view = View::default();
    typed(&mut view, "fix");
    view.key(key(KeyCode::Enter, KeyModifiers::ALT));
    typed(&mut view, "it");
    assert_eq!(
        press(&mut view, KeyCode::Enter),
        Some(Out::Command(Command::Prompt {
            text: "fix\nit".to_owned(),
            attachments: Vec::new(),
        }))
    );
    assert!(view.composer.text.is_empty());
    assert_eq!(press(&mut view, KeyCode::Enter), None);
}

#[test]
fn editing_keys_move_by_characters() {
    let mut view = View::default();
    typed(&mut view, "héllo wörld");
    press(&mut view, KeyCode::Left);
    press(&mut view, KeyCode::Backspace);
    assert_eq!(view.composer.text, "héllo wörd");
    ctrl(&mut view, 'w');
    assert_eq!(view.composer.text, "héllo d");
    press(&mut view, KeyCode::Home);
    press(&mut view, KeyCode::Delete);
    assert_eq!(view.composer.text, "éllo d");
    ctrl(&mut view, 'e');
    ctrl(&mut view, 'u');
    assert_eq!(view.composer.text, "");
}

#[test]
fn paste_normalises_line_endings() {
    let mut view = View::default();
    view.paste("a\r\nb\rc");
    assert_eq!(view.composer.text, "a\nb\nc");
}

#[test]
fn permission_keys_answer_the_oldest_question() {
    let mut view = View::default();
    snapshot(&mut view, asking());
    press(&mut view, KeyCode::Down);
    assert_eq!(
        press(&mut view, KeyCode::Enter),
        Some(Out::Command(Command::Answer {
            option: "deny".to_owned(),
            permission: PermissionId(3)
        }))
    );
    assert_eq!(
        press(&mut view, KeyCode::Char('1')),
        Some(Out::Command(Command::Answer {
            option: "allow".to_owned(),
            permission: PermissionId(3)
        }))
    );
    assert_eq!(press(&mut view, KeyCode::Char('9')), None);
    // Typing still works and queues instead of answering.
    typed(&mut view, "x");
    assert!(matches!(
        press(&mut view, KeyCode::Enter),
        Some(Out::Command(Command::Prompt { .. }))
    ));
}

#[test]
fn ctrl_c_clears_then_cancels_then_quits_on_a_second_press() {
    let mut view = View::default();
    snapshot(
        &mut view,
        Transcript {
            status: Status::Working,
            ..Transcript::new("Claude")
        },
    );
    typed(&mut view, "draft");
    assert_eq!(ctrl(&mut view, 'c'), None);
    assert!(view.composer.text.is_empty());
    assert_eq!(ctrl(&mut view, 'c'), Some(Out::Command(Command::Cancel)));
    snapshot(&mut view, Transcript::new("Claude"));
    assert_eq!(ctrl(&mut view, 'c'), None);
    assert!(view.flash.is_some());
    assert_eq!(ctrl(&mut view, 'c'), Some(Out::Quit));
    typed(&mut view, "x");
    assert_eq!(ctrl(&mut view, 'd'), None, "Ctrl+D quits only when empty");
}

#[test]
fn shift_tab_cycles_modes() {
    let mut view = View::default();
    let modes = Modes {
        available: ["a", "b"]
            .into_iter()
            .map(|id| Mode {
                description: None,
                id: id.to_owned(),
                name: id.to_owned(),
            })
            .collect(),
        current: "b".to_owned(),
    };
    snapshot(
        &mut view,
        Transcript {
            modes: Some(modes),
            ..Transcript::new("Claude")
        },
    );
    assert_eq!(
        press(&mut view, KeyCode::BackTab),
        Some(Out::Command(Command::SetMode { id: "a".to_owned() }))
    );
}

#[test]
fn a_gap_or_misfit_asks_for_a_snapshot() {
    let mut view = View::default();
    let cursor = snapshot(&mut view, Transcript::new("Claude"));
    let push = |id| Change::Push {
        entry: Entry {
            id: EntryId(id),
            kind: EntryKind::Agent {
                text: "x".to_owned(),
            },
        },
    };
    let next = Cursor {
        seq: cursor.seq + 1,
        ..cursor
    };
    assert_eq!(
        view.receive(ServerMessage::Change {
            change: push(1),
            cursor: next
        }),
        Received::Applied
    );
    let skipped = Cursor {
        seq: cursor.seq + 3,
        ..cursor
    };
    assert_eq!(
        view.receive(ServerMessage::Change {
            change: push(2),
            cursor: skipped
        }),
        Received::Resync
    );
    assert_eq!(view.cursor(), None);

    let cursor = snapshot(&mut view, Transcript::new("Claude"));
    let next = Cursor {
        seq: cursor.seq + 1,
        ..cursor
    };
    let misfit = Change::Append {
        id: EntryId(9),
        text: "x".to_owned(),
    };
    assert_eq!(
        view.receive(ServerMessage::Change {
            change: misfit,
            cursor: next
        }),
        Received::Resync
    );
}

#[test]
fn cached_lines_follow_appends() {
    let mut view = View::default();
    let cursor = snapshot(&mut view, Transcript::new("Claude"));
    let changes = [
        Change::Push {
            entry: Entry {
                id: EntryId(1),
                kind: EntryKind::Agent {
                    text: "one".to_owned(),
                },
            },
        },
        Change::Append {
            id: EntryId(1),
            text: " two".to_owned(),
        },
    ];
    for (offset, change) in changes.into_iter().enumerate() {
        view.lines(20);
        view.receive(ServerMessage::Change {
            change,
            cursor: Cursor {
                seq: cursor.seq + 1 + offset as u64,
                ..cursor
            },
        });
    }
    let text: Vec<String> = view
        .lines(20)
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect()
        })
        .collect();
    assert_eq!(text[0], "  one two");
}

#[test]
fn the_composer_layout_places_the_cursor() {
    let mut view = View::default();
    typed(&mut view, "abcdef");
    assert_eq!(
        view.composer.layout(4),
        (vec!["abcd".to_owned(), "ef".to_owned()], (1, 2))
    );
    typed(&mut view, "gh");
    assert_eq!(view.composer.layout(4).1, (2, 0));
    press(&mut view, KeyCode::Home);
    assert_eq!(view.composer.layout(4).1, (0, 0));
    view.paste("x\n");
    assert_eq!(view.composer.layout(4).1, (1, 0));
}

#[test]
fn program_status_follows_the_transcript() {
    assert_eq!(program_status(&asking()), "blocked:kind=permission");
    assert_eq!(program_status(&Transcript::default()), "working");
    let idle = Transcript {
        status: Status::Idle,
        ..Transcript::default()
    };
    assert_eq!(program_status(&idle), "idle");
}
