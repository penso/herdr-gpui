use super::*;
use crate::hub::{ChatRow, Layout, Summary, View, Workspace};
use crate::web::Indicators;
use herdr_chat_protocol::{
    Entry, EntryId, EntryKind, Permission, PermissionId, PermissionKind, PermissionOption,
    Transcript,
};
use herdr_client::protocol::AgentStatus;
use std::collections::HashMap;

mod ansi;
mod chat_view;
mod icons;
mod lists;
mod markup;
mod picker;
mod preview;
mod search;
mod settings;
mod terminal_view;

pub(super) fn row(pane: &str) -> ChatRow {
    ChatRow {
        pane: pane.into(),
        tab: "t1".into(),
        endpoint: "0123456789abcdef0123456789abcdef".parse().unwrap(),
        agent: Some("claude".into()),
        title: "Remote plan".into(),
        status: AgentStatus::Working,
    }
}

pub(super) fn workspace(chats: Vec<ChatRow>) -> Workspace {
    Workspace {
        id: "w1".into(),
        label: "herdr-gpui".into(),
        branch: Some("main".into()),
        cwd: "/code/herdr-gpui".into(),
        chats,
        terminals: vec![crate::hub::layout_terminal("t2", "zsh")],
    }
}

pub(super) fn question(title: &str) -> Permission {
    Permission {
        call_id: None,
        id: PermissionId(7),
        title: title.into(),
        options: vec![
            PermissionOption {
                id: "deny".into(),
                kind: PermissionKind::RejectOnce,
                name: "Deny".into(),
            },
            PermissionOption {
                id: "always".into(),
                kind: PermissionKind::AllowAlways,
                name: "Always allow".into(),
            },
            PermissionOption {
                id: "once".into(),
                kind: PermissionKind::AllowOnce,
                name: "Allow once".into(),
            },
        ],
    }
}

pub(super) fn entry(id: u64, kind: EntryKind) -> Entry {
    Entry {
        id: EntryId(id),
        kind,
    }
}
