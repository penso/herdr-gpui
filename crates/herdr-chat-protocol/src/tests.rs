#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

mod agents;
mod assist;
mod auto_accept;
mod change;
mod display;
mod endpoint;
mod frame;
mod media;
mod notices;
mod wire;

fn text(id: u64, kind: fn(String) -> EntryKind, value: &str) -> Entry {
    Entry {
        id: EntryId(id),
        kind: kind(value.to_owned()),
    }
}

fn agent(id: u64, value: &str) -> Entry {
    text(id, |text| EntryKind::Agent { text }, value)
}

fn user(id: u64, value: &str) -> Entry {
    text(id, |text| EntryKind::User { text, files: 0 }, value)
}

fn tool(id: u64, status: ToolStatus) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::Tool(Tool {
            call_id: "call-1".to_owned(),
            content: Vec::new(),
            kind: ToolKind::Execute,
            locations: Vec::new(),
            status,
            title: "cargo test".to_owned(),
        }),
    }
}

fn permission(id: u64) -> Permission {
    Permission {
        call_id: Some("call-1".to_owned()),
        id: PermissionId(id),
        options: vec![PermissionOption {
            id: "allow".to_owned(),
            kind: PermissionKind::AllowOnce,
            name: "Allow".to_owned(),
        }],
        title: "Run cargo test?".to_owned(),
    }
}
