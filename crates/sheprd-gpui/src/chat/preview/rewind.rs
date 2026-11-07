//! The `rewind` cases: a prompt's "Edit from here", and the bar over the
//! composer once it is chosen. `rewind` shows the pencil, as with the keyboard
//! in the chat, and `rewind-unavailable` its dimmed form; the others edit the
//! second prompt with Keep files, Restore files, an agent working in the
//! workspace, and no checkpoint.

use std::sync::Arc;

use gpui::{Context, Window};
use sheprd_chat_protocol::{Entry, EntryId, EntryKind, SessionInfo};
use sheprd_client::protocol::AgentStatus;

use super::PANE;
use crate::{
    SheprdWindow,
    chat::rewind::{Checkpoint, Files},
    window::chrome_focus::Region,
};

const EDITED: EntryId = EntryId(3);

pub(super) fn entries() -> Vec<Entry> {
    let user = |id, text: &str| Entry {
        id: EntryId(id),
        kind: EntryKind::User {
            text: text.into(),
            files: 0,
        },
    };
    let agent = |id, text: &str| Entry {
        id: EntryId(id),
        kind: EntryKind::Agent { text: text.into() },
    };
    vec![
        user(1, "Why does the export skip the last row?"), // i18n: data
        agent(2, "`rows.len() - 1` drops it, so end at `rows.len()`."), // i18n: data
        user(3, "Rewrite the whole exporter to stream rows"), // i18n: data
        agent(4, "Done: `export.rs` now streams rows."),   // i18n: data
    ]
}

pub(super) fn session(case: &str) -> SessionInfo {
    SessionInfo {
        cwd: None,
        id: "synthetic-preview-session".into(),
        rewind: case != "rewind-unavailable",
        title: None,
    }
}

fn pencil(case: &str) -> bool {
    matches!(case, "rewind" | "rewind-unavailable")
}

/// Keeps the keyboard in the chat for the cases that show the pencil: the
/// window taking focus as it opens hands the region back to the terminal.
pub(super) fn hold_region(view: &mut SheprdWindow, case: &str) {
    if pencil(case) {
        view.chrome_region = Some(Region::Chat);
    }
}

/// Puts the window in the state `case` shows.
pub(super) fn enter(
    view: &mut SheprdWindow,
    case: &str,
    window: &mut Window,
    cx: &mut Context<SheprdWindow>,
) {
    if pencil(case) {
        return;
    }
    view.begin_chat_rewind(PANE, EDITED, window, cx);
    if case == "rewind-working"
        && let Some(snapshot) = view.live.snapshot.as_mut()
    {
        Arc::make_mut(snapshot).agents[0].agent_status = AgentStatus::Working;
    }
    if let Some(rewind) = view
        .chats
        .pane_mut(PANE)
        .and_then(|pane| pane.rewind.as_mut())
    {
        rewind.checkpoint = if case == "rewind-missing" {
            Checkpoint::Missing
        } else {
            Checkpoint::Found("preview".into())
        };
        if matches!(case, "rewind-restore" | "rewind-working") {
            rewind.files = Files::Restore;
        }
    }
}
