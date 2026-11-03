//! Reducer vectors: a start transcript, changes, and the transcript the
//! protocol's own reducer ends with, which the Swift port must match. A
//! rejected vector names the error its last change fails with instead.

use super::fixtures::check;
use herdr_chat_protocol::{
    Author, Change, Choice, ConfigOption, Cost, Entry, EntryId, EntryKind, Error, Image, ImageType,
    Mode, Modes, NoticeLevel, OptionCategory, OptionKind, PermissionId, PlanEntry, PlanPriority,
    PlanStatus, QuestionAnswer, Queued, SessionInfo, SlashCommand, Status, Tokens, Tool,
    ToolContent, ToolKind, ToolStatus, Transcript, Turn, TurnEnd, TurnSummary, Usage,
};
use serde::Serialize;

#[derive(Serialize)]
struct Vector {
    start: Transcript,
    changes: Vec<Change>,
    end: Transcript,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<&'static str>,
}

/// Applies `changes` to `start`: every one must apply, except that a
/// rejected vector's last must fail and leave the transcript as it was.
fn vector(start: Transcript, changes: Vec<Change>, rejected: bool) -> Vector {
    let mut end = start.clone();
    let mut error = None;
    for (index, change) in changes.iter().enumerate() {
        let before = end.clone();
        match end.apply(change) {
            Ok(()) => assert!(
                !(rejected && index + 1 == changes.len()),
                "{change:?} applied"
            ),
            Err(failure) => {
                assert!(
                    rejected && index + 1 == changes.len(),
                    "{change:?}: {failure}"
                );
                assert_eq!(end, before, "a rejected change left a trace");
                error = Some(name(&failure));
            }
        }
    }
    Vector {
        start,
        changes,
        end,
        error,
    }
}

/// The name a vector gives a reducer error.
fn name(error: &Error) -> &'static str {
    match error {
        Error::DuplicatePermission(_) => "duplicate_permission",
        Error::EntryOrder { .. } => "entry_order",
        Error::NoModes => "no_modes",
        Error::NotText(_) => "not_text",
        Error::QuestionAnswersLimit(_) => "question_answers_limit",
        Error::UnknownEntry(_) => "unknown_entry",
        Error::UnknownPermission(_) => "unknown_permission",
        _ => "other",
    }
}

fn entry(id: u64, kind: EntryKind) -> Entry {
    Entry {
        id: EntryId(id),
        kind,
    }
}

fn agent(id: u64, text: &str) -> Entry {
    entry(id, EntryKind::Agent { text: text.into() })
}

fn tool(status: ToolStatus) -> Tool {
    Tool {
        call_id: "call-1".into(),
        content: vec![
            ToolContent::Text {
                text: "cargo nextest run".into(),
            },
            ToolContent::Diff {
                new: "let share = 1.0;\n".into(),
                old: Some("let share = 0.5;\n".into()),
                path: "src/splits.rs".into(),
            },
            ToolContent::Diff {
                new: "fn new_file() {}\n".into(),
                old: None,
                path: "src/new.rs".into(),
            },
            ToolContent::Image {
                image: Image {
                    data: "iVBORw0KGgo=".into(),
                    kind: ImageType::Png,
                },
            },
            ToolContent::Terminal {
                id: "term-1".into(),
            },
        ],
        kind: ToolKind::Execute,
        locations: vec!["src/splits.rs".into()],
        parent: None,
        status,
        title: "Run just ci".into(),
    }
}

fn entries() -> Vector {
    let changes = vec![
        Change::Push {
            entry: entry(
                1,
                EntryKind::User {
                    text: "Look at this\n\u{1f4ce} shot.png".into(),
                    files: 1,
                },
            ),
        },
        Change::Push {
            entry: entry(2, EntryKind::Thought { text: "Hmm".into() }),
        },
        Change::Push {
            entry: agent(3, "Sure"),
        },
        Change::Append {
            id: EntryId(3),
            text: ", done.".into(),
        },
        Change::Push {
            entry: entry(
                4,
                EntryKind::Image {
                    author: Author::Agent,
                    image: Image {
                        data: "R0lGODlhAQABAAAAACw=".into(),
                        kind: ImageType::Gif,
                    },
                },
            ),
        },
        Change::Push {
            entry: entry(5, EntryKind::Tool(tool(ToolStatus::InProgress))),
        },
        Change::Replace {
            entry: entry(5, EntryKind::Tool(tool(ToolStatus::Completed))),
        },
        Change::Push {
            entry: entry(
                6,
                EntryKind::Turn(TurnSummary {
                    ended_ms: None,
                    duration_ms: 6000,
                    end: TurnEnd::Done,
                    tokens: Some(Tokens {
                        cached: 10,
                        input: 100,
                        output: 50,
                        thought: 5,
                        total: 165,
                    }),
                }),
            ),
        },
        Change::Push {
            entry: entry(
                7,
                EntryKind::Notice {
                    answers: vec![QuestionAnswer {
                        question: "Which layout?".into(),
                        answer: "Split".into(),
                    }],
                    details: Some("from the transcript".into()),
                    level: NoticeLevel::Warning,
                    text: "Answered".into(),
                },
            ),
        },
        Change::Append {
            id: EntryId(7),
            text: " earlier".into(),
        },
        Change::Truncate { before: EntryId(3) },
    ];
    vector(Transcript::new("Claude"), changes, false)
}

fn permissions() -> Vector {
    let changes = vec![
        Change::PermissionAsked {
            permission: super::permission(1),
        },
        Change::PermissionAsked {
            permission: super::permission(2),
        },
        Change::PermissionResolved {
            id: PermissionId(1),
        },
        Change::AutoAccept { on: true },
    ];
    vector(Transcript::new("Codex"), changes, false)
}

fn session() -> Vector {
    let changes = vec![
        Change::Status {
            status: Status::Working,
        },
        Change::Status {
            status: Status::Failed {
                message: "the agent stopped".into(),
            },
        },
        Change::Plan {
            plan: vec![
                PlanEntry {
                    content: "Read".into(),
                    priority: PlanPriority::High,
                    status: PlanStatus::Completed,
                },
                PlanEntry {
                    content: "Edit".into(),
                    priority: PlanPriority::Medium,
                    status: PlanStatus::InProgress,
                },
                PlanEntry {
                    content: "Test".into(),
                    priority: PlanPriority::Low,
                    status: PlanStatus::Pending,
                },
            ],
        },
        Change::Modes {
            modes: Some(Modes {
                available: vec![
                    Mode {
                        description: Some("Ask before edits".into()),
                        id: "default".into(),
                        name: "Default".into(),
                    },
                    Mode {
                        description: None,
                        id: "bypass".into(),
                        name: "Bypass permissions".into(),
                    },
                ],
                current: "default".into(),
            }),
        },
        Change::CurrentMode {
            id: "bypass".into(),
        },
        Change::Options {
            options: vec![
                ConfigOption {
                    category: OptionCategory::Model,
                    description: None,
                    id: "model".into(),
                    kind: OptionKind::Select {
                        choices: vec![Choice {
                            description: Some("Most capable".into()),
                            group: Some("Claude".into()),
                            id: "opus".into(),
                            name: "Opus 5.5".into(),
                        }],
                        current: "opus".into(),
                    },
                    name: "Model".into(),
                },
                ConfigOption {
                    category: OptionCategory::ThoughtLevel,
                    description: Some("Think longer".into()),
                    id: "thinking".into(),
                    kind: OptionKind::Toggle { current: true },
                    name: "Thinking".into(),
                },
            ],
        },
        Change::Turn {
            turn: Some(Turn {
                started_ms: 1_761_670_000_000,
            }),
        },
        Change::Queue {
            queued: vec![Queued {
                attachments: vec![herdr_chat_protocol::Attachment {
                    path: "/tmp/shot.png".into(),
                }],
                id: herdr_chat_protocol::CommandId(uuid::Uuid::from_u128(7)),
                text: "and the docs".into(),
            }],
        },
        Change::Clients { count: 2 },
        Change::Session {
            session: Some(SessionInfo {
                cwd: Some("/work".into()),
                id: "s1".into(),
                title: Some("Equalize".into()),
            }),
        },
        Change::Commands {
            commands: vec![SlashCommand {
                description: "Review the branch".into(),
                hint: Some("[focus]".into()),
                name: "review".into(),
            }],
        },
        Change::Usage {
            usage: Some(Usage {
                cost: Some(Cost {
                    currency: "USD".into(),
                    micros: 840_000,
                }),
                size: 200_000,
                used: 84_000,
            }),
        },
        Change::Turn { turn: None },
        Change::AutoAccept { on: false },
    ];
    vector(Transcript::new("Claude"), changes, false)
}

/// A transcript with an agent entry 1, a tool entry 2, and permission 1.
fn base() -> Transcript {
    let mut transcript = Transcript::new("Claude");
    transcript.entries = vec![
        agent(1, "hi"),
        entry(2, EntryKind::Tool(tool(ToolStatus::Pending))),
    ];
    transcript.permissions = vec![super::permission(1)];
    transcript
}

#[test]
fn reducer_vectors_match_the_swift_fixtures() {
    check("reducer/entries", &entries());
    check("reducer/permissions", &permissions());
    check("reducer/session", &session());
    let rejected = [
        (
            "unknown_entry",
            Change::Append {
                id: EntryId(9),
                text: "x".into(),
            },
        ),
        (
            "entry_order",
            Change::Push {
                entry: agent(2, "late"),
            },
        ),
        (
            "not_text",
            Change::Append {
                id: EntryId(2),
                text: "x".into(),
            },
        ),
        (
            "unknown_permission",
            Change::PermissionResolved {
                id: PermissionId(9),
            },
        ),
        (
            "duplicate_permission",
            Change::PermissionAsked {
                permission: super::permission(1),
            },
        ),
        ("no_modes", Change::CurrentMode { id: "plan".into() }),
    ];
    for (name, change) in rejected {
        let vector = vector(
            base(),
            vec![
                Change::Append {
                    id: EntryId(1),
                    text: " there".into(),
                },
                change,
            ],
            true,
        );
        assert_eq!(vector.error, Some(name));
        check(&format!("reducer/rejected_{name}"), &vector);
    }
}
