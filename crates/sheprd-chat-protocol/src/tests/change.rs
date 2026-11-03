use super::*;

#[test]
fn streamed_chunks_append_to_their_entry() {
    let mut transcript = Transcript::new("Claude");
    transcript
        .apply(&Change::Push {
            entry: user(1, "hi"),
        })
        .unwrap();
    transcript
        .apply(&Change::Push {
            entry: agent(2, "Hel"),
        })
        .unwrap();
    transcript
        .apply(&Change::Append {
            id: EntryId(2),
            text: "lo".to_owned(),
        })
        .unwrap();
    assert_eq!(
        transcript.entry(EntryId(2)).unwrap().kind.text(),
        Some("Hello")
    );
    assert_eq!(
        transcript.entry(EntryId(1)).unwrap().kind.text(),
        Some("hi")
    );
}

#[test]
fn entries_must_arrive_in_order() {
    let mut transcript = Transcript::new("Claude");
    transcript
        .apply(&Change::Push {
            entry: agent(5, ""),
        })
        .unwrap();
    let error = transcript
        .apply(&Change::Push {
            entry: agent(5, ""),
        })
        .unwrap_err();
    assert!(matches!(
        error,
        Error::EntryOrder {
            id: EntryId(5),
            last: EntryId(5)
        }
    ));
    assert_eq!(transcript.entries.len(), 1);
}

#[test]
fn a_change_that_does_not_fit_leaves_the_transcript_alone() {
    let mut transcript = Transcript::new("Claude");
    transcript
        .apply(&Change::Push {
            entry: tool(1, ToolStatus::Pending),
        })
        .unwrap();
    let before = transcript.clone();
    assert!(matches!(
        transcript.apply(&Change::Append {
            id: EntryId(1),
            text: "x".to_owned()
        }),
        Err(Error::NotText(EntryId(1)))
    ));
    assert!(matches!(
        transcript.apply(&Change::Replace {
            entry: agent(9, "")
        }),
        Err(Error::UnknownEntry(EntryId(9)))
    ));
    assert!(matches!(
        transcript.apply(&Change::CurrentMode {
            id: "plan".to_owned()
        }),
        Err(Error::NoModes)
    ));
    assert_eq!(transcript, before);
}

#[test]
fn tool_updates_replace_the_whole_entry() {
    let mut transcript = Transcript::new("Codex");
    transcript
        .apply(&Change::Push {
            entry: tool(1, ToolStatus::Pending),
        })
        .unwrap();
    transcript
        .apply(&Change::Replace {
            entry: tool(1, ToolStatus::Completed),
        })
        .unwrap();
    let EntryKind::Tool(tool) = &transcript.entries[0].kind else {
        panic!("tool entry changed kind");
    };
    assert_eq!(tool.status, ToolStatus::Completed);
}

#[test]
fn truncation_drops_only_older_entries() {
    let mut transcript = Transcript::new("Claude");
    for id in 1..=4 {
        transcript
            .apply(&Change::Push {
                entry: agent(id, ""),
            })
            .unwrap();
    }
    transcript
        .apply(&Change::Truncate { before: EntryId(3) })
        .unwrap();
    let ids: Vec<_> = transcript.entries.iter().map(|entry| entry.id.0).collect();
    assert_eq!(ids, [3, 4]);
    assert!(transcript.truncated);
    assert!(transcript.entry(EntryId(3)).is_some());
}

#[test]
fn permissions_are_asked_once_and_resolved_once() {
    let mut transcript = Transcript::new("Claude");
    transcript
        .apply(&Change::PermissionAsked {
            permission: permission(1),
        })
        .unwrap();
    assert!(transcript.is_blocked());
    assert!(matches!(
        transcript.apply(&Change::PermissionAsked {
            permission: permission(1)
        }),
        Err(Error::DuplicatePermission(PermissionId(1)))
    ));
    transcript
        .apply(&Change::PermissionResolved {
            id: PermissionId(1),
        })
        .unwrap();
    assert!(!transcript.is_blocked());
    assert!(matches!(
        transcript.apply(&Change::PermissionResolved {
            id: PermissionId(1)
        }),
        Err(Error::UnknownPermission(PermissionId(1)))
    ));
}

#[test]
fn current_mode_changes_within_known_modes() {
    let mut transcript = Transcript::new("Claude");
    transcript
        .apply(&Change::Modes {
            modes: Some(Modes {
                available: vec![
                    Mode {
                        description: None,
                        id: "default".to_owned(),
                        name: "Default".to_owned(),
                    },
                    Mode {
                        description: None,
                        id: "plan".to_owned(),
                        name: "Plan".to_owned(),
                    },
                ],
                current: "default".to_owned(),
            }),
        })
        .unwrap();
    transcript
        .apply(&Change::CurrentMode {
            id: "plan".to_owned(),
        })
        .unwrap();
    assert_eq!(
        transcript.modes.as_ref().unwrap().current().unwrap().name,
        "Plan"
    );
}
