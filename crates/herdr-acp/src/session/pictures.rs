//! Pictures the agent points at, added to the transcript as it goes: the
//! files an agent message links as Markdown images follow that message
//! once it closes, and a finished tool call shows the images it wrote or
//! read. See [`crate::pictures`] for what counts as one.

use herdr_chat_protocol::{Author, Change, EntryId, EntryKind};

use super::{Session, Update};

/// What the session remembers so each message and tool call is looked at
/// once.
#[derive(Debug, Default)]
pub(super) struct Pictures {
    /// The agent message still open, whose links are read when it closes.
    open: Option<EntryId>,
    /// Tool calls already looked at.
    tools: std::collections::HashSet<String>,
}

impl<R: FnMut(Update)> Session<R> {
    /// Before `change` lands: when it starts an entry after an agent
    /// message, the pictures that message linked go between them, and the
    /// entry takes an id after theirs, as ids only grow.
    pub(super) fn pictures_before(&mut self, change: Change) -> Change {
        let Change::Push { mut entry } = change else {
            return change;
        };
        if self.pictures.open == Some(entry.id) {
            return Change::Push { entry };
        }
        if let Some(open) = self.pictures.open.take() {
            let linked = self
                .transcript
                .entry(open)
                .and_then(|entry| match &entry.kind {
                    EntryKind::Agent { text } => {
                        Some(crate::pictures::linked(text, &self.config.cwd))
                    }
                    _ => None,
                })
                .unwrap_or_default();
            let mut added = false;
            for image in linked.iter().filter_map(|path| crate::pictures::read(path)) {
                let picture = self.projector.image(Author::Agent, image);
                self.emit(picture);
                added = true;
            }
            if added {
                entry.id = self.projector.renumber(entry.id);
            }
        }
        if matches!(entry.kind, EntryKind::Agent { .. }) {
            self.pictures.open = Some(entry.id);
        }
        Change::Push { entry }
    }

    /// After `change` landed: a tool call it finished shows the images it
    /// wrote or read.
    pub(super) fn pictures_after(&mut self, change: &Change) {
        let (Change::Push { entry } | Change::Replace { entry }) = change else {
            return;
        };
        let EntryKind::Tool(tool) = &entry.kind else {
            return;
        };
        if tool.status != herdr_chat_protocol::ToolStatus::Completed
            || !self.pictures.tools.insert(tool.call_id.clone())
        {
            return;
        }
        let images = crate::pictures::of_tool(tool, &self.config.cwd);
        if images.is_empty() {
            return;
        }
        let mut entry = entry.clone();
        if let EntryKind::Tool(tool) = &mut entry.kind {
            tool.content.extend(images);
        }
        self.emit(Change::Replace { entry });
    }
}
