//! The daemon request a catalog command becomes against a snapshot.

use crate::Command;
use serde_json::{Value, json};
use sheprd_client::{Method, protocol::ClientShellSnapshot};

pub fn request(command: Command, snapshot: &ClientShellSnapshot) -> Option<(Method, Value)> {
    let workspace = snapshot
        .workspaces
        .iter()
        .find(|w| Some(&w.workspace_id) == snapshot.focused_workspace_id.as_ref());
    let tab = workspace.and_then(|w| {
        snapshot.tabs.iter().find(|t| {
            Some(&t.tab_id) == snapshot.focused_tab_id.as_ref() && t.workspace_id == w.workspace_id
        })
    });
    let pane = tab.and_then(|t| {
        snapshot.panes.iter().find(|p| {
            Some(&p.pane_id) == snapshot.focused_pane_id.as_ref()
                && p.tab_id == t.tab_id
                && p.workspace_id == t.workspace_id
        })
    });
    Some(match command {
        Command::Workspace => {
            let mut params = json!({"focus": true});
            if snapshot.focused_workspace_id.is_some() {
                params["source_workspace_id"] = json!(workspace?.workspace_id);
            }
            (Method::WorkspaceCreate, params)
        }
        Command::Tab => (
            Method::TabCreate,
            json!({"workspace_id": workspace?.workspace_id, "focus": true}),
        ),
        Command::SplitRight | Command::SplitDown => (
            Method::PaneSplit,
            json!({
                "target_pane_id": pane?.pane_id,
                "direction": if matches!(command, Command::SplitRight) { "right" } else { "down" },
                "focus": true,
            }),
        ),
        Command::NextTab | Command::PreviousTab => {
            let workspace = &workspace?.workspace_id;
            let tabs: Vec<_> = snapshot
                .tabs
                .iter()
                .filter(|t| &t.workspace_id == workspace)
                .collect();
            let index = tabs
                .iter()
                .position(|t| Some(&t.tab_id) == snapshot.focused_tab_id.as_ref())?;
            let next = if matches!(command, Command::NextTab) {
                (index + 1) % tabs.len()
            } else {
                (index + tabs.len() - 1) % tabs.len()
            };
            (Method::TabFocus, json!({"tab_id": tabs[next].tab_id}))
        }
        Command::FocusLeft | Command::FocusRight | Command::FocusUp | Command::FocusDown => (
            Method::PaneFocusDirection,
            json!({"pane_id": pane?.pane_id, "direction": direction(command)?}),
        ),
        // Herdr nudges the split by its own default step, as its TUI does.
        Command::ResizeLeft | Command::ResizeRight | Command::ResizeUp | Command::ResizeDown => (
            Method::PaneResize,
            json!({"pane_id": pane?.pane_id, "direction": direction(command)?}),
        ),
        Command::SwapLeft | Command::SwapRight | Command::SwapUp | Command::SwapDown => (
            Method::PaneSwap,
            json!({"pane_id": pane?.pane_id, "direction": direction(command)?}),
        ),
        Command::MoveTabPrevious | Command::MoveTabNext => {
            let tab = tab?;
            let tabs: Vec<_> = snapshot
                .tabs
                .iter()
                .filter(|t| t.workspace_id == tab.workspace_id)
                .collect();
            if tabs.len() <= 1 {
                return None;
            }
            let source = tabs.iter().position(|t| t.tab_id == tab.tab_id)?;
            // As Herdr's TUI moves it: the index counts the moving tab still
            // in place, and a tab at either end wraps around to the other.
            let insert_index = if command == Command::MoveTabNext {
                if source + 1 == tabs.len() {
                    0
                } else {
                    source + 2
                }
            } else if source == 0 {
                tabs.len()
            } else {
                source - 1
            };
            (
                Method::TabMove,
                json!({"tab_id": tab.tab_id, "insert_index": insert_index}),
            )
        }
        Command::NextPane | Command::PreviousPane => {
            let pane = pane?;
            let panes: Vec<_> = snapshot
                .panes
                .iter()
                .filter(|p| p.workspace_id == pane.workspace_id && p.tab_id == pane.tab_id)
                .collect();
            // Finding the current pane also guarantees a nonempty cycle.
            let index = panes.iter().position(|p| p.pane_id == pane.pane_id)?;
            let next = if command == Command::NextPane {
                (index + 1) % panes.len()
            } else {
                (index + panes.len() - 1) % panes.len()
            };
            (Method::PaneFocus, json!({"pane_id": panes[next].pane_id}))
        }
        Command::Zoom => (
            Method::PaneZoom,
            json!({"pane_id": pane?.pane_id, "mode": "toggle"}),
        ),
        Command::ClearPane => (Method::PaneClear, json!({"pane_id": pane?.pane_id})),
        Command::EditScrollback => (
            Method::PaneEditScrollback,
            json!({"pane_id": pane?.pane_id}),
        ),
        Command::ClosePane => (Method::PaneClose, json!({"pane_id": pane?.pane_id})),
        Command::CloseTab => (Method::TabClose, json!({"tab_id": tab?.tab_id})),
        // By position, as Herdr's `switch_tab` does: a tab's `number` is an
        // ID that is never reused, so it drifts from the strip once a tab closes.
        Command::TabNumber(number) => {
            let workspace = workspace?;
            let target = snapshot
                .tabs
                .iter()
                .filter(|t| t.workspace_id == workspace.workspace_id)
                .nth(usize::from(number).checked_sub(1)?)?;
            (Method::TabFocus, json!({"tab_id": target.tab_id}))
        }
        Command::NewWindow
        | Command::NewWorktree
        | Command::OpenWorktree
        | Command::RemoveWorktree
        | Command::WorktreeNotes
        | Command::EditWorktreeNote
        | Command::Find
        | Command::FindNext
        | Command::FindPrevious
        | Command::CopyMode
        | Command::AnnotateSelection
        | Command::ToggleSidebar
        | Command::ToggleStatusBar
        | Command::ToggleModPanel
        | Command::FocusNextRegion
        | Command::FocusPreviousRegion
        | Command::ToggleZenMode
        | Command::TogglePassThrough
        | Command::DevicesOverview
        | Command::NewCoreTerminal
        | Command::MirrorCorePane
        | Command::ConvertToChat
        | Command::IncreaseFontSize
        | Command::DecreaseFontSize
        | Command::ResetFontSize
        | Command::Settings
        | Command::Keybinds
        | Command::Sessions
        | Command::Themes
        | Command::WorkspacePicker
        | Command::Palette
        | Command::ResumePalette
        | Command::GoToSymbol
        | Command::GoToFile
        | Command::Detach
        | Command::Reconnect
        | Command::Quit
        | Command::Logs
        | Command::About
        | Command::OpenNotificationTarget
        | Command::NewBrowserTab
        | Command::OpenOrchestrator
        | Command::ReviewChanges
        | Command::GitMenu
        | Command::ToggleAnnotate
        | Command::InstallBrowserSkill
        | Command::SplitEditor
        | Command::OpenCode
        | Command::OpenPx0
        | Command::RemotePair
        | Command::ToggleFullScreen
        | Command::CycleWindows
        | Command::CloseWindow
        // These need state beyond the snapshot, such as the sidebar's order
        // or a dialog, so the window runs them.
        | Command::RenameTab
        | Command::ReopenClosedTab
        | Command::LastPane
        | Command::Back
        | Command::Forward
        | Command::ResizeMode
        | Command::EqualizeSplits
        | Command::RenamePane
        | Command::MovePane
        | Command::PreviousWorkspace
        | Command::NextWorkspace
        | Command::WorkspaceNumber(_)
        | Command::RenameWorkspace
        | Command::CloseWorkspace
        | Command::PreviousAgent
        | Command::NextAgent
        | Command::AgentNumber(_)
        | Command::ReloadConfig => return None,
    })
}

/// The pane direction a directional command names.
fn direction(command: Command) -> Option<&'static str> {
    Some(match command {
        Command::FocusLeft | Command::SwapLeft | Command::ResizeLeft => "left",
        Command::FocusRight | Command::SwapRight | Command::ResizeRight => "right",
        Command::FocusUp | Command::SwapUp | Command::ResizeUp => "up",
        Command::FocusDown | Command::SwapDown | Command::ResizeDown => "down",
        _ => return None,
    })
}
