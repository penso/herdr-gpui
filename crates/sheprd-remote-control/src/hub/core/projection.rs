//! Core owns terminal records; the desktop owns tab and split membership.

use super::pane_id;
use crate::hub::{Layout, Terminal, TerminalPane, Workspace, layout::TerminalAgent};
use serde::Deserialize;
use sheprd_client::protocol::AgentStatus;
use sheprd_core_client::wire::{StatusState, Terminal as Record};
use std::{collections::HashSet, io::Read, path::Path};

#[derive(Deserialize)]
pub(super) struct Saved {
    tabs: Vec<SavedTab>,
}

#[derive(Deserialize)]
struct SavedTab {
    id: u64,
    location: Option<serde_json::Value>,
    scope: String,
    title: String,
    workspace_id: String,
}

pub(super) fn read(path: &Path) -> Option<Saved> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.len() > 1024 * 1024 {
        return None;
    }
    let saved: Saved = serde_json::from_slice(&bytes).ok()?;
    (saved.tabs.len() <= 256).then_some(saved)
}

fn leaves<'a>(node: &'a serde_json::Value, out: &mut Vec<&'a str>, depth: usize) {
    if depth > 16 || out.len() >= 16 {
        return;
    }
    if let Some(id) = node.as_str() {
        out.push(id);
    } else {
        leaves(&node["first"], out, depth + 1);
        leaves(&node["second"], out, depth + 1);
    }
}

pub(super) fn project(
    boot: &str,
    records: &[Record],
    saved: Option<&Saved>,
    scope: Option<&str>,
) -> Layout {
    let mut layout = Layout {
        boot: format!("core:{boot}"),
        connected: true,
        working: working(records),
        workspaces: Vec::new(),
    };
    let mut assigned = HashSet::new();
    for tab in saved.into_iter().flat_map(|saved| &saved.tabs) {
        if Some(tab.scope.as_str()) != scope || tab.workspace_id.is_empty() {
            continue;
        }
        let Some(location) = &tab.location else {
            continue;
        };
        if location["kind"] != "core_terminal" || location["observe"] == true {
            continue;
        }
        let node = location.get("panes").or_else(|| location.get("terminal"));
        let mut addresses = Vec::new();
        if let Some(node) = node {
            leaves(node, &mut addresses, 0);
        }
        let members: Vec<&Record> = addresses
            .into_iter()
            .filter_map(|address| records.iter().find(|record| record.address == address))
            .filter(|record| assigned.insert(record.address.as_str()))
            .collect();
        if let Some(terminal) = terminal(
            boot,
            &format!("core:tab:{}", tab.id),
            &tab.title,
            &members,
            urgency,
        ) {
            add(&mut layout, &tab.workspace_id, terminal);
        }
    }
    for record in records
        .iter()
        .filter(|record| !assigned.contains(record.address.as_str()))
    {
        let tab = pane_id(boot, &record.address);
        if let Some(terminal) = terminal(boot, &tab, "", &[record], urgency) {
            add(&mut layout, super::UNLISTED, terminal);
        }
    }
    layout
}

fn add(layout: &mut Layout, workspace: &str, terminal: Terminal) {
    if let Some(row) = layout.workspaces.iter_mut().find(|row| row.id == workspace) {
        row.terminals.push(terminal);
        return;
    }
    layout.workspaces.push(Workspace {
        branch: None,
        chats: Vec::new(),
        cwd: terminal.cwd.clone().unwrap_or_default(),
        id: workspace.to_owned(),
        label: label(workspace, terminal.cwd.as_deref()),
        session: None,
        terminals: vec![terminal],
        worktree: None,
    });
}

/// What a phone calls a core workspace. A saved tab's workspace id is an
/// opaque Herdr 1 id, shown only if Herdr 1 no longer lists it: its folder
/// reads better.
fn label(workspace: &str, cwd: Option<&str>) -> String {
    (workspace != super::UNLISTED)
        .then_some(cwd)
        .flatten()
        .and_then(|cwd| Path::new(cwd).file_name()?.to_str())
        .unwrap_or("Herdr 2")
        .to_owned()
}

pub(super) fn status(record: &Record) -> AgentStatus {
    match record.status.as_ref().map(|status| status.state) {
        Some(StatusState::Blocked | StatusState::Error) => AgentStatus::Blocked,
        Some(StatusState::Done) => AgentStatus::Done,
        Some(StatusState::Idle) => AgentStatus::Idle,
        Some(StatusState::Working) => AgentStatus::Working,
        _ => AgentStatus::Unknown,
    }
}

pub(super) fn working(records: &[Record]) -> usize {
    records
        .iter()
        .filter(|record| status(record) == AgentStatus::Working)
        .count()
}

pub(super) fn terminal(
    boot: &str,
    tab: &str,
    title: &str,
    records: &[&Record],
    urgency: impl Fn(AgentStatus) -> u8,
) -> Option<Terminal> {
    let first = *records.first()?;
    let panes = records
        .iter()
        .enumerate()
        .map(|(index, record)| TerminalPane {
            cwd: cwd(record),
            focused: index == 0,
            label: record.name.clone().or_else(|| record.title.clone()),
            pane: pane_id(boot, &record.address),
        })
        .collect();
    let agent = records
        .iter()
        .filter(|record| record.status.is_some())
        .max_by_key(|record| urgency(status(record)))
        .map(|record| TerminalAgent {
            name: record
                .status
                .as_ref()
                .map(|status| status.app.clone())
                .filter(|name| !name.is_empty()),
            pane: pane_id(boot, &record.address),
            status: status(record),
            title: record
                .status
                .as_ref()
                .map(|status| status.title.clone())
                .filter(|title| !title.is_empty())
                .or_else(|| record.title.clone()),
            changed_ms: None,
        });
    Some(Terminal {
        agent,
        cwd: cwd(first),
        label: if title.is_empty() {
            first
                .name
                .as_ref()
                .or(first.title.as_ref())
                .unwrap_or(&first.address)
                .clone()
        } else {
            title.to_owned()
        },
        panes,
        tab: tab.to_owned(),
        zoomed: false,
    })
}

pub(super) fn cwd(record: &Record) -> Option<String> {
    record
        .foreground
        .as_ref()
        .map(|foreground| &foreground.cwd)
        .filter(|cwd| !cwd.is_empty())
        .or(Some(&record.cwd))
        .filter(|cwd| !cwd.is_empty())
        .cloned()
}

fn urgency(status: AgentStatus) -> u8 {
    match status {
        AgentStatus::Blocked => 4,
        AgentStatus::Working => 3,
        AgentStatus::Done => 2,
        AgentStatus::Idle => 1,
        AgentStatus::Unknown => 0,
    }
}
