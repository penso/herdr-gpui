//! Herdr desktop spaces stored in the core's read-only herdr-app document.

mod names;

use super::{id, projection};
use crate::hub::{Layout, Workspace};
use serde::Deserialize;
use serde_json::Value;
use sheprd_client::protocol::AgentStatus;
use sheprd_core_client::wire::Terminal;
use sheprd_session::SavedTabs;
use std::{
    collections::HashSet,
    io::{self, Write},
};

pub(super) use names::Names;
const MAX_BYTES: usize = 1024 * 1024;
const MAX_TABS: usize = 256;

#[derive(Deserialize)]
pub(super) struct Spaces {
    spaces: Vec<Space>,
}

#[derive(Deserialize)]
struct Space {
    folder: String,
    id: String,
    #[serde(default)]
    machine: Option<String>,
    name: String,
    tabs: Vec<Tab>,
}

#[derive(Deserialize)]
struct Tab {
    id: String,
    name: String,
    root: Node,
}

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum Node {
    // keep-sorted start
    Block {
        #[serde(default)]
        content: Content,
    },
    Column {
        children: Vec<Node>,
    },
    Row {
        children: Vec<Node>,
    },
    // keep-sorted end
}

#[derive(Default, Deserialize)]
struct Content {
    terminal: Option<String>,
}

impl Node {
    fn leaves<'a>(&'a self, out: &mut Vec<&'a str>, depth: usize) -> Option<()> {
        if depth > 16 {
            return None;
        }
        match self {
            Self::Block { content } => {
                if let Some(terminal) = &content.terminal {
                    if out.len() >= 16 {
                        return None;
                    }
                    out.push(terminal);
                }
            }
            Self::Column { children } | Self::Row { children } => {
                for child in children {
                    child.leaves(out, depth + 1)?;
                }
            }
        }
        Some(())
    }
}

struct Size(usize);
impl Write for Size {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 += bytes.len();
        if self.0 > MAX_BYTES {
            return Err(io::Error::other("layout exceeds its size limit"));
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Spaces {
    pub(super) fn read(value: Value) -> Option<Self> {
        serde_json::to_writer(Size(0), &value).ok()?;
        let doc: Self = serde_json::from_value(value).ok()?;
        if doc.spaces.len() > MAX_TABS
            || doc
                .spaces
                .iter()
                .map(|space| space.tabs.len())
                .sum::<usize>()
                > MAX_TABS
        {
            return None;
        }
        let mut ids = HashSet::new();
        let mut tabs = HashSet::new();
        for space in &doc.spaces {
            if space.id.is_empty() || !ids.insert(&space.id) {
                return None;
            }
            for tab in &space.tabs {
                if tab.id.is_empty() || !tabs.insert(&tab.id) {
                    return None;
                }
                tab.root.leaves(&mut Vec::new(), 0)?;
            }
        }
        Some(doc)
    }

    /// The document's spaces around the app's own tabs: a terminal a saved
    /// tab holds stays in that tab and its split, even when a space holds it
    /// too, and spaces list the rest.
    pub(super) fn project(
        &self,
        boot: &str,
        records: &[Terminal],
        saved: Option<&SavedTabs>,
        scope: Option<&str>,
        names: &mut Names,
    ) -> Layout {
        let mut layout = Layout {
            boot: format!("core:{boot}"),
            connected: true,
            working: 0,
            workspaces: Vec::new(),
        };
        let held: HashSet<&str> = saved
            .zip(scope)
            .into_iter()
            .flat_map(|(saved, scope)| saved.core_tabs(scope))
            .flat_map(|tab| tab.panes.terminals())
            .map(|terminal| terminal.as_str())
            .collect();
        let mut assigned = HashSet::new();
        // A local space's terminals a saved tab holds instead. A local space
        // vouches for them, so another machine's space cannot omit them.
        let mut vouched = HashSet::new();
        // Another machine's terminals are omitted, not unlisted: a phone
        // must not reach them through the Herdr 2 group.
        let mut omitted = HashSet::new();
        for space in &self.spaces {
            if space.id.contains('/')
                || space
                    .machine
                    .as_ref()
                    .is_some_and(|machine| !machine.is_empty())
            {
                for tab in &space.tabs {
                    let mut addresses = Vec::new();
                    let _ = tab.root.leaves(&mut addresses, 0);
                    omitted.extend(addresses);
                }
                continue;
            }
            let mut workspace = Workspace {
                branch: None,
                chats: Vec::new(),
                cwd: space.folder.clone(),
                id: format!("core:space:{}", id(&space.id)),
                label: space.name.clone(),
                session: None,
                terminals: Vec::new(),
                worktree: None,
            };
            let mut place = None;
            for (index, tab) in space.tabs.iter().enumerate() {
                let mut addresses = Vec::new();
                let _ = tab.root.leaves(&mut addresses, 0);
                let (saved_here, addresses): (Vec<_>, Vec<_>) = addresses
                    .into_iter()
                    .partition(|address| held.contains(address));
                vouched.extend(saved_here);
                let members = addresses
                    .into_iter()
                    .filter_map(|address| records.iter().find(|record| record.address == address))
                    .filter(|record| assigned.insert(record.address.as_str()))
                    .collect::<Vec<_>>();
                if place.is_none() {
                    place = members.first().and_then(|record| projection::cwd(record));
                }
                let title = if tab.name.is_empty() {
                    (index + 1).to_string()
                } else {
                    tab.name.clone()
                };
                if let Some(terminal) = projection::terminal(
                    boot,
                    &format!("core:app-tab:{}", id(&tab.id)),
                    &title,
                    &members,
                    urgency,
                ) {
                    workspace.terminals.push(terminal);
                }
            }
            if workspace.label.is_empty() {
                workspace.label = names.name(place.as_deref().unwrap_or(&space.folder));
            }
            layout.workspaces.push(workspace);
        }
        let shown = records
            .iter()
            .filter(|record| {
                let address = record.address.as_str();
                assigned.contains(address)
                    || vouched.contains(address)
                    || !omitted.contains(address)
            })
            .cloned()
            .collect::<Vec<_>>();
        layout.working = projection::working(&shown);
        let unlisted = shown
            .into_iter()
            .filter(|record| !assigned.contains(record.address.as_str()))
            .collect::<Vec<_>>();
        layout
            .workspaces
            .extend(projection::project(boot, &unlisted, saved, scope).workspaces);
        layout
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

// The Herdr desktop keeps finished work visible above work in progress.
fn urgency(status: AgentStatus) -> u8 {
    match status {
        AgentStatus::Blocked => 4,
        AgentStatus::Done => 3,
        AgentStatus::Working => 2,
        AgentStatus::Idle => 1,
        AgentStatus::Unknown => 0,
    }
}
