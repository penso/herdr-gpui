//! The `steps` and `steps-open` cases: an edit and a long command running at
//! once, for the working row, with every step in full in the second case.
//! Run them with `HERDR_TEST_RICH_CHAT_WORKING` set.

use sheprd_chat_protocol::{Entry, EntryId, EntryKind, Tool, ToolKind, ToolStatus};

fn step(id: u64, kind: ToolKind, location: &str, started_ms: u64, title: &str) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::Tool(Tool {
            call_id: format!("call-{id}"),
            content: Vec::new(),
            kind,
            locations: vec![location.into()],
            parent: None,
            started_ms: Some(started_ms),
            status: ToolStatus::InProgress,
            title: title.into(),
        }),
    }
}

pub(super) fn entries(now_ms: u64) -> Vec<Entry> {
    vec![
        Entry {
            id: EntryId(1),
            kind: EntryKind::User {
                text: "Keep the purpose type private.".into(), // i18n: data
                files: 0,
            },
        },
        step(
            2,
            ToolKind::Execute,
            "/Users/me/worktrees/server", // i18n: data
            now_ms.saturating_sub(9_000),
            "`sed -i '' 's/^export type ElementAgentPurpose = /type ElementAgentPurpose = /' apps/server/src/element-agent/purpose.ts`", // i18n: data
        ),
        step(
            3,
            ToolKind::Edit,
            "/Users/me/worktrees/server/apps/server/src/element-agent/purpose.ts", // i18n: data
            now_ms.saturating_sub(4_000),
            "Edit apps/server/src/element-agent/purpose.ts", // i18n: data
        ),
    ]
}
