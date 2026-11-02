//! What the lock screen's answer buttons and step stack need.

use super::*;
use herdr_chat_protocol::{PermissionKind, PermissionOption};

fn asking(id: u64, options: usize) -> Permission {
    Permission {
        options: (0..options)
            .map(|n| PermissionOption {
                id: format!("o{n}"),
                kind: PermissionKind::AllowOnce,
                name: format!("Option {n}"),
            })
            .collect(),
        ..permission(id, "Run just ci")
    }
}

#[test]
fn a_question_carries_its_id_and_first_three_options() {
    let live = decide(
        &running(),
        &Change::PermissionAsked {
            permission: asking(4, 5),
        },
    )
    .unwrap();
    assert_eq!(live.state.permission, Some(4));
    let ids: Vec<&str> = live.state.options.iter().map(|o| o.id.as_str()).collect();
    assert_eq!(ids, ["o0", "o1", "o2"]);
}

#[test]
fn a_step_while_blocked_keeps_the_waiting_question_buttons() {
    let mut before = running();
    before.permissions = vec![asking(2, 2)];
    let live = decide(
        &before,
        &push(5, tool("cargo test", ToolStatus::InProgress)),
    )
    .unwrap();
    assert_eq!(live.state.phase, Phase::Question);
    assert_eq!(live.state.permission, Some(2));
    assert_eq!(live.state.options.len(), 2);
}

#[test]
fn working_has_no_buttons() {
    let live = decide(
        &running(),
        &push(5, tool("cargo test", ToolStatus::InProgress)),
    )
    .unwrap();
    assert_eq!(live.state.permission, None);
    assert!(live.state.options.is_empty());
}

#[test]
fn a_new_step_stacks_the_two_before_it_oldest_first() {
    let mut before = running();
    for (id, title) in [(2, "Read lib.rs"), (3, "Edit lib.rs"), (4, "Edit main.rs")] {
        before.entries.push(Entry {
            id: EntryId(id),
            kind: tool(title, ToolStatus::Completed),
        });
    }
    let live = decide(
        &before,
        &push(5, tool("cargo test", ToolStatus::InProgress)),
    )
    .unwrap();
    assert_eq!(live.state.step, "cargo test");
    assert_eq!(live.state.previous, ["Edit lib.rs", "Edit main.rs"]);
}

#[test]
fn a_turn_start_has_no_previous_steps() {
    let mut before = idle();
    before.entries.push(Entry {
        id: EntryId(2),
        kind: tool("Old step", ToolStatus::Completed),
    });
    before.entries.push(Entry {
        id: EntryId(3),
        kind: EntryKind::User {
            text: "next".to_owned(),
            files: 0,
        },
    });
    let live = decide(
        &before,
        &Change::Turn {
            turn: Some(Turn {
                started_ms: STARTED_MS,
            }),
        },
    )
    .unwrap();
    assert!(live.state.previous.is_empty());
}
