//! The `running` and `running-open` cases: a sub-agent whose steps show in
//! its card, a second one still exploring, and a dev server left running in
//! the background after the turn, with the table of them open in the
//! second case.

use herdr_chat_protocol::{
    Activity, ActivityKind, ActivityState, Entry, EntryId, EntryKind, Tool, ToolContent, ToolKind,
    ToolStatus,
};

fn tool(
    id: u64,
    call: &str,
    kind: ToolKind,
    status: ToolStatus,
    title: &str,
    parent: Option<&str>,
) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::Tool(Tool {
            call_id: call.into(),
            content: Vec::new(),
            kind,
            locations: Vec::new(),
            parent: parent.map(str::to_owned),
            status,
            title: title.into(),
        }),
    }
}

pub(super) fn entries() -> Vec<Entry> {
    let mut server = tool(
        8,
        "bash-dev",
        ToolKind::Execute,
        ToolStatus::Completed,
        "`npm run dev -- --port 5173`",
        None,
    );
    if let EntryKind::Tool(tool) = &mut server.kind {
        tool.content = vec![ToolContent::Text {
            text: "Command running in background with ID: b3x9k2.".into(),
        }];
    }
    vec![
        Entry {
            id: EntryId(1),
            kind: EntryKind::User {
                text: "Check why the attach socket drops after a daemon restart, and keep the dev server up so I can try it.".into(),
                files: 0,
            },
        },
        Entry {
            id: EntryId(2),
            kind: EntryKind::Agent {
                text: "I'll split this up: one sub-agent explores the reconnect path while another reviews the reducer tests.".into(),
            },
        },
        tool(3, "task-explore", ToolKind::Subagent, ToolStatus::Completed, "Explore how the attach socket reconnects after a daemon restart", None),
        tool(4, "read-1", ToolKind::Read, ToolStatus::Completed, "Read crates/herdr-chat-protocol/src/endpoint.rs", Some("task-explore")),
        tool(5, "grep-1", ToolKind::Search, ToolStatus::Completed, "Search for fn reconnect", Some("task-explore")),
        tool(6, "bash-1", ToolKind::Execute, ToolStatus::Failed, "`cargo test -p herdr-client reconnect`", Some("task-explore")),
        tool(7, "task-review", ToolKind::Subagent, ToolStatus::InProgress, "Review the reducer tests", None),
        server,
        Entry {
            id: EntryId(9),
            kind: EntryKind::Agent {
                text: "The socket is reopened before the daemon's new endpoint exists. The dev server keeps running in the background on port 5173.".into(),
            },
        },
    ]
}

pub(super) fn activities(now_ms: u64) -> Vec<Activity> {
    let activity =
        |call: &str, kind, label: &str, started: u64, ended: Option<u64>, state| Activity {
            call_id: call.into(),
            detail: None,
            finished_ms: ended.map(|ago| now_ms.saturating_sub(ago)),
            kind,
            label: label.into(),
            started_ms: now_ms.saturating_sub(started),
            state,
        };
    vec![
        activity(
            "task-explore",
            ActivityKind::Subagent,
            "Explore how the attach socket reconnects after a daemon restart",
            190_000,
            Some(120_000),
            ActivityState::Finished,
        ),
        Activity {
            detail: Some("Search for apply(&Change".into()),
            ..activity(
                "task-review",
                ActivityKind::Subagent,
                "Review the reducer tests",
                38_000,
                None,
                ActivityState::Running,
            )
        },
        activity(
            "bash-dev",
            ActivityKind::Shell,
            "npm run dev -- --port 5173",
            252_000,
            None,
            ActivityState::Running,
        ),
        Activity {
            detail: Some("Exited 1".into()),
            ..activity(
                "bash-watch",
                ActivityKind::Shell,
                "pnpm vitest --watch",
                400_000,
                Some(230_000),
                ActivityState::Failed,
            )
        },
    ]
}
