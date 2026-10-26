use super::*;
use crate::push::triggers::{EXCERPT_CHARS, excerpt, tag};
use herdr_chat_protocol::{
    Change, Cost, Entry, EntryId, EntryKind, NoticeLevel, Permission, PermissionId, Status,
    Transcript, TurnEnd, TurnSummary, Usage,
};

const CHAT: Chat<'static> = Chat {
    agent: "Claude",
    pane: "w1-p3",
    title: "remote plan",
    workspace: "herdr-gpui",
};

fn permission(id: u64, title: &str) -> Permission {
    Permission {
        call_id: None,
        id: PermissionId(id),
        options: Vec::new(),
        title: title.to_owned(),
    }
}

fn push(kind: EntryKind) -> Change {
    Change::Push {
        entry: Entry {
            id: EntryId(10),
            kind,
        },
    }
}

fn transcript_with_answer(text: &str) -> Transcript {
    let mut transcript = Transcript::new("Claude");
    transcript.status = Status::Working;
    transcript.entries.push(Entry {
        id: EntryId(1),
        kind: EntryKind::Agent {
            text: text.to_owned(),
        },
    });
    transcript.entries.push(Entry {
        id: EntryId(2),
        kind: EntryKind::Thought {
            text: "thinking".to_owned(),
        },
    });
    transcript.usage = Some(Usage {
        cost: Some(Cost {
            currency: "USD".to_owned(),
            micros: 420_000,
        }),
        size: 200_000,
        used: 1_000,
    });
    transcript
}

fn turn(end: TurnEnd) -> Change {
    push(EntryKind::Turn(TurnSummary {
        duration_ms: 12_000,
        end,
        tokens: None,
    }))
}

#[test]
fn a_permission_is_an_urgent_question() {
    let before = Transcript::new("Claude");
    let change = Change::PermissionAsked {
        permission: permission(1, "Run rm -rf target"),
    };
    let generic = decide(&CHAT, &before, &change, Privacy::Generic).unwrap();
    assert_eq!(generic.kind, Kind::Question);
    assert_eq!(generic.urgency, Urgency::High);
    assert!(!generic.clear);
    assert_eq!(generic.title, "Claude needs an answer");
    assert_eq!(generic.body, "herdr-gpui \u{b7} remote plan");
    assert_eq!(generic.tag, "chat-w1-p3");

    let excerpt = decide(&CHAT, &before, &change, Privacy::Excerpt).unwrap();
    assert_eq!(
        excerpt.body,
        "Run rm -rf target\nherdr-gpui \u{b7} remote plan"
    );
}

#[test]
fn auto_accepted_permissions_stay_quiet() {
    let mut before = Transcript::new("Claude");
    before.auto_accept = true;
    let change = Change::PermissionAsked {
        permission: permission(1, "Edit"),
    };
    assert_eq!(decide(&CHAT, &before, &change, Privacy::Generic), None);
}

#[test]
fn resolving_the_last_permission_clears_the_notification() {
    let mut before = Transcript::new("Claude");
    before.permissions = vec![permission(1, "a"), permission(2, "b")];
    let resolve = |id| Change::PermissionResolved {
        id: PermissionId(id),
    };
    assert_eq!(decide(&CHAT, &before, &resolve(1), Privacy::Generic), None);

    before.permissions.remove(0);
    let clear = decide(&CHAT, &before, &resolve(2), Privacy::Generic).unwrap();
    assert!(clear.clear);
    assert_eq!(clear.tag, tag(CHAT.pane));
    assert!(clear.title.is_empty() && clear.body.is_empty());
    // An unknown id is nothing to clear.
    assert_eq!(decide(&CHAT, &before, &resolve(9), Privacy::Generic), None);
}

#[test]
fn a_finished_turn_reports_its_summary() {
    let before = transcript_with_answer("All   tests\npass.");
    let generic = decide(&CHAT, &before, &turn(TurnEnd::Done), Privacy::Generic).unwrap();
    assert_eq!(generic.kind, Kind::TurnEnded);
    assert_eq!(generic.urgency, Urgency::Normal);
    assert_eq!(generic.title, "Claude finished");
    assert_eq!(
        generic.body,
        "herdr-gpui \u{b7} remote plan\nWorked 12s \u{b7} $0.42"
    );
    assert!(!generic.body.contains("tests"));

    let excerpt = decide(&CHAT, &before, &turn(TurnEnd::Done), Privacy::Excerpt).unwrap();
    assert_eq!(
        excerpt.body,
        "All tests pass.\nherdr-gpui \u{b7} remote plan\nWorked 12s \u{b7} $0.42"
    );
}

#[test]
fn a_cancelled_turn_stays_quiet() {
    let before = transcript_with_answer("partial");
    assert_eq!(
        decide(&CHAT, &before, &turn(TurnEnd::Cancelled), Privacy::Generic),
        None
    );
    assert!(decide(&CHAT, &before, &turn(TurnEnd::Failed), Privacy::Generic).is_some());
}

#[test]
fn an_error_notice_is_an_error() {
    let before = Transcript::new("Claude");
    let change = push(EntryKind::Notice {
        answers: Vec::new(),
        details: None,
        level: NoticeLevel::Error,
        text: "Rate limited by the API".to_owned(),
    });
    let generic = decide(&CHAT, &before, &change, Privacy::Generic).unwrap();
    assert_eq!(generic.kind, Kind::Error);
    assert_eq!(generic.title, "Claude hit an error");
    assert!(!generic.body.contains("Rate"));
    let excerpt = decide(&CHAT, &before, &change, Privacy::Excerpt).unwrap();
    assert!(excerpt.body.starts_with("Rate limited by the API\n"));

    let warning = push(EntryKind::Notice {
        answers: Vec::new(),
        details: None,
        level: NoticeLevel::Warning,
        text: "slow".to_owned(),
    });
    assert_eq!(decide(&CHAT, &before, &warning, Privacy::Generic), None);
}

#[test]
fn failing_or_quitting_while_asking_is_an_error() {
    let mut before = Transcript::new("Claude");
    before.status = Status::Working;
    let failed = Change::Status {
        status: Status::Failed {
            message: "crashed".to_owned(),
        },
    };
    let exited = Change::Status {
        status: Status::Exited {
            message: "bye".to_owned(),
        },
    };
    assert_eq!(
        decide(&CHAT, &before, &failed, Privacy::Generic).map(|n| n.kind),
        Some(Kind::Error)
    );
    assert_eq!(decide(&CHAT, &before, &exited, Privacy::Generic), None);

    before.permissions.push(permission(1, "Edit"));
    let stopped = decide(&CHAT, &before, &exited, Privacy::Generic).unwrap();
    assert_eq!(stopped.title, "Claude stopped while asking");
}

#[test]
fn ordinary_changes_stay_quiet() {
    let before = transcript_with_answer("Hello");
    let quiet = [
        Change::Append {
            id: EntryId(1),
            text: " world".to_owned(),
        },
        push(EntryKind::Agent {
            text: "More".to_owned(),
        }),
        Change::Status {
            status: Status::Working,
        },
        Change::Status {
            status: Status::Idle,
        },
        Change::Clients { count: 2 },
        Change::Turn { turn: None },
    ];
    for change in quiet {
        assert_eq!(
            decide(&CHAT, &before, &change, Privacy::Excerpt),
            None,
            "{change:?}"
        );
    }
}

#[test]
fn excerpts_are_one_short_line() {
    assert_eq!(excerpt("  a\n\nb\tc  "), "a b c");
    let long = "word ".repeat(100);
    let cut = excerpt(&long);
    assert!(cut.chars().count() <= EXCERPT_CHARS);
    assert!(cut.ends_with("word\u{2026}"), "{cut}");
    let exact = "x".repeat(EXCERPT_CHARS);
    assert_eq!(excerpt(&exact), exact);
}

#[test]
fn messages_link_to_the_chat() {
    let before = Transcript::new("Claude");
    let chat = Chat {
        pane: "a b/c",
        ..CHAT
    };
    let change = Change::PermissionAsked {
        permission: permission(1, "Edit"),
    };
    let message = decide(&chat, &before, &change, Privacy::Generic)
        .unwrap()
        .message(3);
    assert_eq!(message.url, "/chat/a%20b%2Fc");
    assert_eq!(message.badge, 3);
    let json = serde_json::to_value(&message).unwrap();
    assert_eq!(json["kind"], "question");
    assert_eq!(json["clear"], false);
    assert_eq!(json["tag"], "chat-a b/c");
    for key in ["title", "body"] {
        assert!(json[key].is_string(), "{key}");
    }
}
