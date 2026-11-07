//! The `state-*` cases: a chat at each point the header and the working row
//! tell apart. `state-waiting` is a turn stopped on a permission request; the
//! others are an idle agent after a turn that ended one way or another.

use sheprd_chat_protocol::{
    Entry, EntryId, EntryKind, Permission, PermissionId, PermissionKind, PermissionOption, Status,
    Tokens, Transcript, Turn, TurnEnd, TurnSummary,
};

fn ended(end: TurnEnd) -> Entry {
    Entry {
        id: EntryId(3),
        kind: EntryKind::Turn(TurnSummary {
            duration_ms: 42_000,
            ended_ms: None,
            end,
            tokens: Some(Tokens {
                input: 18_600,
                output: 450,
                ..Tokens::default()
            }),
        }),
    }
}

pub(super) fn apply(case: &str, transcript: &mut Transcript, now_ms: u64) {
    let end = match case {
        "state-completed" => TurnEnd::Done,
        "state-failed" => TurnEnd::Failed,
        "state-stopped" => TurnEnd::Cancelled,
        _ => {
            transcript.status = Status::Working;
            transcript.turn = Some(Turn {
                started_ms: now_ms.saturating_sub(12_000),
            });
            transcript.permissions = vec![Permission {
                call_id: None,
                id: PermissionId(1),
                options: vec![PermissionOption {
                    id: "allow".into(),
                    kind: PermissionKind::AllowOnce,
                    name: "Allow once".into(), // i18n: data
                }],
                title: "Run cargo test -p sheprd-gpui".into(), // i18n: data
            }];
            return;
        }
    };
    transcript.status = Status::Idle;
    transcript.entries.push(ended(end));
}
