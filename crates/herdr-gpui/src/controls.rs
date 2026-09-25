use herdr_client::{Method, protocol::ClientShellSnapshot};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Deserialize)]
pub enum Command {
    NewWindow,
    Workspace,
    NewWorktree,
    Tab,
    SplitRight,
    SplitDown,
    NextTab,
    PreviousTab,
    FocusLeft,
    FocusRight,
    FocusUp,
    FocusDown,
    NextPane,
    PreviousPane,
    Zoom,
    ClearPane,
    ClosePane,
    CloseTab,
    TabNumber(u8),
    ToggleSidebar,
    IncreaseFontSize,
    DecreaseFontSize,
    ResetFontSize,
    Settings,
    Keybinds,
    Sessions,
    Themes,
    WorkspacePicker,
    Palette,
    Reconnect,
    Quit,
    Logs,
    About,
    OpenNotificationTarget,
}

pub struct CommandInfo {
    pub command: Command,
    /// The key naming this command in the config file's `[keybindings]`.
    pub name: &'static str,
    pub label: &'static str,
    /// Default keystrokes, primary first. The config can replace each list.
    pub shortcuts: &'static [&'static str],
}

pub const COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        command: Command::OpenNotificationTarget,
        name: "open_notification_target",
        label: "Open Notification Target",
        shortcuts: &["cmd-alt-n"],
    },
    CommandInfo {
        command: Command::Logs,
        name: "logs",
        label: "Logs",
        shortcuts: &[],
    },
    CommandInfo {
        command: Command::NewWindow,
        name: "new_window",
        label: "New Window",
        shortcuts: &["cmd-alt-shift-n"],
    },
    CommandInfo {
        command: Command::Workspace,
        name: "new_workspace",
        label: "New Workspace",
        shortcuts: &["cmd-shift-n"],
    },
    CommandInfo {
        command: Command::NewWorktree,
        name: "new_worktree",
        label: "New Worktree",
        shortcuts: &["cmd-n"],
    },
    CommandInfo {
        command: Command::Tab,
        name: "new_tab",
        label: "New Tab",
        shortcuts: &["cmd-t"],
    },
    CommandInfo {
        command: Command::SplitRight,
        name: "split_right",
        label: "Split Right",
        shortcuts: &["cmd-d"],
    },
    CommandInfo {
        command: Command::SplitDown,
        name: "split_down",
        label: "Split Down",
        shortcuts: &["cmd-shift-d"],
    },
    CommandInfo {
        command: Command::NextTab,
        name: "next_tab",
        label: "Next Tab",
        shortcuts: &["cmd-shift-]"],
    },
    CommandInfo {
        command: Command::PreviousTab,
        name: "previous_tab",
        label: "Previous Tab",
        shortcuts: &["cmd-shift-["],
    },
    CommandInfo {
        command: Command::FocusLeft,
        name: "focus_left",
        label: "Focus Left",
        shortcuts: &["cmd-alt-left"],
    },
    CommandInfo {
        command: Command::FocusRight,
        name: "focus_right",
        label: "Focus Right",
        shortcuts: &["cmd-alt-right"],
    },
    CommandInfo {
        command: Command::FocusUp,
        name: "focus_up",
        label: "Focus Up",
        shortcuts: &["cmd-alt-up"],
    },
    CommandInfo {
        command: Command::FocusDown,
        name: "focus_down",
        label: "Focus Down",
        shortcuts: &["cmd-alt-down"],
    },
    CommandInfo {
        command: Command::NextPane,
        name: "next_pane",
        label: "Next Pane",
        shortcuts: &["cmd-alt-]"],
    },
    CommandInfo {
        command: Command::PreviousPane,
        name: "previous_pane",
        label: "Previous Pane",
        shortcuts: &["cmd-alt-["],
    },
    CommandInfo {
        command: Command::Zoom,
        name: "toggle_zoom",
        label: "Toggle Pane Zoom",
        shortcuts: &["cmd-shift-enter"],
    },
    CommandInfo {
        command: Command::ClearPane,
        name: "clear_pane",
        label: "Clear Pane",
        shortcuts: &["cmd-k"],
    },
    CommandInfo {
        command: Command::ClosePane,
        name: "close_pane",
        label: "Close Pane",
        shortcuts: &["cmd-w"],
    },
    CommandInfo {
        command: Command::CloseTab,
        name: "close_tab",
        label: "Close Tab",
        shortcuts: &["cmd-shift-w"],
    },
    CommandInfo {
        command: Command::TabNumber(1),
        name: "focus_tab_1",
        label: "Focus Tab 1",
        shortcuts: &["cmd-1"],
    },
    CommandInfo {
        command: Command::TabNumber(2),
        name: "focus_tab_2",
        label: "Focus Tab 2",
        shortcuts: &["cmd-2"],
    },
    CommandInfo {
        command: Command::TabNumber(3),
        name: "focus_tab_3",
        label: "Focus Tab 3",
        shortcuts: &["cmd-3"],
    },
    CommandInfo {
        command: Command::TabNumber(4),
        name: "focus_tab_4",
        label: "Focus Tab 4",
        shortcuts: &["cmd-4"],
    },
    CommandInfo {
        command: Command::TabNumber(5),
        name: "focus_tab_5",
        label: "Focus Tab 5",
        shortcuts: &["cmd-5"],
    },
    CommandInfo {
        command: Command::TabNumber(6),
        name: "focus_tab_6",
        label: "Focus Tab 6",
        shortcuts: &["cmd-6"],
    },
    CommandInfo {
        command: Command::TabNumber(7),
        name: "focus_tab_7",
        label: "Focus Tab 7",
        shortcuts: &["cmd-7"],
    },
    CommandInfo {
        command: Command::TabNumber(8),
        name: "focus_tab_8",
        label: "Focus Tab 8",
        shortcuts: &["cmd-8"],
    },
    CommandInfo {
        command: Command::TabNumber(9),
        name: "focus_tab_9",
        label: "Focus Tab 9",
        shortcuts: &["cmd-9"],
    },
    CommandInfo {
        command: Command::ToggleSidebar,
        name: "toggle_sidebar",
        label: "Toggle Sidebar",
        shortcuts: &["cmd-b"],
    },
    CommandInfo {
        command: Command::IncreaseFontSize,
        name: "increase_font_size",
        label: "Increase Font Size",
        shortcuts: &["cmd-=", "cmd-+"],
    },
    CommandInfo {
        command: Command::DecreaseFontSize,
        name: "decrease_font_size",
        label: "Decrease Font Size",
        shortcuts: &["cmd--"],
    },
    CommandInfo {
        command: Command::ResetFontSize,
        name: "reset_font_size",
        label: "Reset Font Size",
        shortcuts: &["cmd-0"],
    },
    CommandInfo {
        command: Command::Settings,
        name: "settings",
        label: "Settings",
        shortcuts: &["cmd-,"],
    },
    CommandInfo {
        command: Command::Keybinds,
        name: "keybindings",
        label: "Keyboard Shortcuts",
        shortcuts: &["cmd-/"],
    },
    CommandInfo {
        command: Command::Sessions,
        name: "sessions",
        label: "Sessions",
        shortcuts: &["cmd-shift-s"],
    },
    CommandInfo {
        command: Command::Themes,
        name: "themes",
        label: "Themes",
        shortcuts: &[],
    },
    CommandInfo {
        command: Command::WorkspacePicker,
        name: "workspace_picker",
        label: "Go To",
        shortcuts: &["cmd-p"],
    },
    CommandInfo {
        command: Command::Palette,
        name: "command_palette",
        label: "Command Palette",
        shortcuts: &["cmd-shift-p"],
    },
    CommandInfo {
        command: Command::Reconnect,
        name: "reconnect",
        label: "Reconnect",
        shortcuts: &[],
    },
    CommandInfo {
        command: Command::Quit,
        name: "quit",
        label: "Quit",
        shortcuts: &["cmd-q"],
    },
    CommandInfo {
        command: Command::About,
        name: "about",
        label: "About Herdr",
        shortcuts: &[],
    },
];

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
        Command::FocusLeft | Command::FocusRight | Command::FocusUp | Command::FocusDown => {
            let direction = match command {
                Command::FocusLeft => "left",
                Command::FocusRight => "right",
                Command::FocusUp => "up",
                Command::FocusDown => "down",
                _ => unreachable!(),
            };
            (
                Method::PaneFocusDirection,
                json!({"pane_id": pane?.pane_id, "direction": direction}),
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
        Command::ClosePane => (Method::PaneClose, json!({"pane_id": pane?.pane_id})),
        Command::CloseTab => (Method::TabClose, json!({"tab_id": tab?.tab_id})),
        Command::TabNumber(number) => {
            let workspace = workspace?;
            let target = snapshot.tabs.iter().find(|t| {
                t.workspace_id == workspace.workspace_id && t.number == usize::from(number)
            })?;
            (Method::TabFocus, json!({"tab_id": target.tab_id}))
        }
        Command::NewWindow
        | Command::NewWorktree
        | Command::ToggleSidebar
        | Command::IncreaseFontSize
        | Command::DecreaseFontSize
        | Command::ResetFontSize
        | Command::Settings
        | Command::Keybinds
        | Command::Sessions
        | Command::Themes
        | Command::WorkspacePicker
        | Command::Palette
        | Command::Reconnect
        | Command::Quit
        | Command::Logs
        | Command::About
        | Command::OpenNotificationTarget => return None,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn snapshot() -> ClientShellSnapshot {
        serde_json::from_str(include_str!(
            "../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
        ))
        .unwrap()
    }

    #[test]
    fn catalog_has_all_native_commands_and_gpui_shortcuts() {
        use Command::*;
        let expected: [(Command, &[&str]); 42] = [
            (OpenNotificationTarget, &["cmd-alt-n"]),
            (Logs, &[]),
            (NewWindow, &["cmd-alt-shift-n"]),
            (Workspace, &["cmd-shift-n"]),
            (NewWorktree, &["cmd-n"]),
            (Tab, &["cmd-t"]),
            (SplitRight, &["cmd-d"]),
            (SplitDown, &["cmd-shift-d"]),
            (NextTab, &["cmd-shift-]"]),
            (PreviousTab, &["cmd-shift-["]),
            (FocusLeft, &["cmd-alt-left"]),
            (FocusRight, &["cmd-alt-right"]),
            (FocusUp, &["cmd-alt-up"]),
            (FocusDown, &["cmd-alt-down"]),
            (NextPane, &["cmd-alt-]"]),
            (PreviousPane, &["cmd-alt-["]),
            (Zoom, &["cmd-shift-enter"]),
            (ClearPane, &["cmd-k"]),
            (ClosePane, &["cmd-w"]),
            (CloseTab, &["cmd-shift-w"]),
            (TabNumber(1), &["cmd-1"]),
            (TabNumber(2), &["cmd-2"]),
            (TabNumber(3), &["cmd-3"]),
            (TabNumber(4), &["cmd-4"]),
            (TabNumber(5), &["cmd-5"]),
            (TabNumber(6), &["cmd-6"]),
            (TabNumber(7), &["cmd-7"]),
            (TabNumber(8), &["cmd-8"]),
            (TabNumber(9), &["cmd-9"]),
            (ToggleSidebar, &["cmd-b"]),
            (IncreaseFontSize, &["cmd-=", "cmd-+"]),
            (DecreaseFontSize, &["cmd--"]),
            (ResetFontSize, &["cmd-0"]),
            (Settings, &["cmd-,"]),
            (Keybinds, &["cmd-/"]),
            (Sessions, &["cmd-shift-s"]),
            (Themes, &[]),
            (WorkspacePicker, &["cmd-p"]),
            (Palette, &["cmd-shift-p"]),
            (Reconnect, &[]),
            (Quit, &["cmd-q"]),
            (About, &[]),
        ];
        assert_eq!(COMMANDS.len(), expected.len());
        let shortcuts: std::collections::HashSet<_> =
            COMMANDS.iter().flat_map(|info| info.shortcuts).collect();
        assert_eq!(
            shortcuts.len(),
            COMMANDS
                .iter()
                .map(|info| info.shortcuts.len())
                .sum::<usize>()
        );
        let names: std::collections::HashSet<_> = COMMANDS.iter().map(|info| info.name).collect();
        assert_eq!(names.len(), COMMANDS.len());
        for (info, (command, shortcuts)) in COMMANDS.iter().zip(expected) {
            assert_eq!(info.command, command);
            assert_eq!(info.shortcuts, shortcuts);
            assert!(!info.name.is_empty());
            assert!(!info.label.is_empty());
            let value = match command {
                TabNumber(number) => json!({"TabNumber": number}),
                _ => json!(format!("{command:?}")),
            };
            assert_eq!(serde_json::from_value::<Command>(value).unwrap(), command);
        }
    }

    /// `cmd--` is the one shortcut whose key is itself the separator, so it
    /// exercises a parser branch no other entry reaches. Binding an unparseable
    /// keystroke would fail at startup rather than here.
    #[test]
    fn every_catalog_shortcut_parses_as_a_keystroke() {
        for shortcut in COMMANDS.iter().flat_map(|info| info.shortcuts) {
            let keystroke = gpui::Keystroke::parse(shortcut)
                .unwrap_or_else(|error| panic!("{shortcut}: {error}"));
            assert!(keystroke.modifiers.platform, "{shortcut}");
        }
        let minus = gpui::Keystroke::parse("cmd--").unwrap();
        assert_eq!(minus.key, "-");
        assert!(!minus.modifiers.shift);
    }

    #[test]
    fn gui_commands_never_send_daemon_requests() {
        let s = snapshot();
        for command in [
            Command::OpenNotificationTarget,
            Command::Logs,
            Command::NewWindow,
            Command::NewWorktree,
            Command::ToggleSidebar,
            Command::IncreaseFontSize,
            Command::DecreaseFontSize,
            Command::ResetFontSize,
            Command::Settings,
            Command::Keybinds,
            Command::Sessions,
            Command::Themes,
            Command::WorkspacePicker,
            Command::Palette,
            Command::Reconnect,
            Command::Quit,
            Command::About,
        ] {
            assert!(request(command, &s).is_none(), "{command:?}");
        }
    }

    #[test]
    fn directional_focus_zoom_and_close_use_explicit_ids() {
        let s = snapshot();
        for (command, direction) in [
            (Command::FocusLeft, "left"),
            (Command::FocusRight, "right"),
            (Command::FocusUp, "up"),
            (Command::FocusDown, "down"),
        ] {
            assert_eq!(
                request(command, &s),
                Some((
                    Method::PaneFocusDirection,
                    json!({"pane_id": s.focused_pane_id, "direction": direction})
                ))
            );
        }
        assert_eq!(
            request(Command::Zoom, &s),
            Some((
                Method::PaneZoom,
                json!({"pane_id": s.focused_pane_id, "mode": "toggle"})
            ))
        );
        assert_eq!(
            request(Command::ClearPane, &s),
            Some((Method::PaneClear, json!({"pane_id": s.focused_pane_id})))
        );
        assert_eq!(
            request(Command::ClosePane, &s),
            Some((Method::PaneClose, json!({"pane_id": s.focused_pane_id})))
        );
        assert_eq!(
            request(Command::CloseTab, &s),
            Some((Method::TabClose, json!({"tab_id": s.focused_tab_id})))
        );
    }

    #[test]
    fn pane_actions_reject_missing_removed_and_foreign_focus() {
        for case in 0..12 {
            let mut s = snapshot();
            match case {
                0 => s.focused_workspace_id = None,
                1 => s.focused_workspace_id = Some("removed".into()),
                2 => s.workspaces.clear(),
                3 => s.focused_tab_id = None,
                4 => s.focused_tab_id = Some("removed".into()),
                5 => s.tabs.clear(),
                6 => s.tabs[0].workspace_id = "foreign".into(),
                7 => s.focused_pane_id = None,
                8 => s.focused_pane_id = Some("removed".into()),
                9 => s.panes.clear(),
                10 => s.panes[0].workspace_id = "foreign".into(),
                11 => s.panes[0].tab_id = "foreign".into(),
                _ => unreachable!(),
            }
            for command in [
                Command::FocusLeft,
                Command::FocusRight,
                Command::FocusUp,
                Command::FocusDown,
                Command::NextPane,
                Command::PreviousPane,
                Command::Zoom,
                Command::ClearPane,
                Command::ClosePane,
                Command::SplitRight,
                Command::SplitDown,
            ] {
                assert!(request(command, &s).is_none(), "case {case}: {command:?}");
            }
            if case < 7 {
                for command in [Command::CloseTab, Command::NextTab, Command::PreviousTab] {
                    assert!(request(command, &s).is_none(), "case {case}: {command:?}");
                }
            }
            if case < 3 {
                assert!(request(Command::TabNumber(1), &s).is_none());
            }
        }
    }

    #[test]
    fn pane_cycle_uses_snapshot_order_within_current_tab_and_workspace() {
        let mut s = snapshot();
        let first = s.panes[0].clone();
        let mut second = first.clone();
        second.pane_id = "second".into();
        let mut third = first.clone();
        third.pane_id = "third".into();
        let mut other_tab = first.clone();
        other_tab.pane_id = "other-tab-pane".into();
        other_tab.tab_id = "other-tab".into();
        let mut other_workspace = first.clone();
        other_workspace.pane_id = "other-workspace-pane".into();
        other_workspace.workspace_id = "other-workspace".into();
        s.panes = vec![first.clone(), other_tab, second, other_workspace, third];
        for (focus, next, previous) in [
            (first.pane_id.as_str(), "second", "third"),
            ("second", "third", first.pane_id.as_str()),
            ("third", first.pane_id.as_str(), "second"),
        ] {
            s.focused_pane_id = Some(focus.into());
            for (command, target) in [(Command::NextPane, next), (Command::PreviousPane, previous)]
            {
                assert_eq!(
                    request(command, &s),
                    Some((Method::PaneFocus, json!({"pane_id": target})))
                );
            }
        }
        s.panes.truncate(1);
        s.focused_pane_id = Some(first.pane_id.clone());
        for command in [Command::NextPane, Command::PreviousPane] {
            assert_eq!(
                request(command, &s),
                Some((Method::PaneFocus, json!({"pane_id": first.pane_id})))
            );
        }
    }

    #[test]
    fn numbered_tabs_use_numbers_not_positions_and_stay_in_workspace() {
        let mut s = snapshot();
        let mut tab = s.tabs[0].clone();
        tab.number = 7;
        let mut second = tab.clone();
        second.number = 2;
        second.tab_id = "second".into();
        let mut foreign = second.clone();
        foreign.workspace_id = "foreign".into();
        foreign.tab_id = "foreign".into();
        s.tabs = vec![foreign, tab.clone(), second];
        assert_eq!(
            request(Command::TabNumber(7), &s),
            Some((Method::TabFocus, json!({"tab_id": tab.tab_id})))
        );
        assert_eq!(
            request(Command::TabNumber(2), &s),
            Some((Method::TabFocus, json!({"tab_id": "second"})))
        );
        for number in [0, 1, 3, 9, 255] {
            assert!(request(Command::TabNumber(number), &s).is_none());
        }
        s.tabs.pop();
        assert!(request(Command::TabNumber(2), &s).is_none());
        // Numeric selection needs a valid workspace, not a current tab or pane.
        s.focused_tab_id = None;
        s.focused_pane_id = None;
        assert!(request(Command::TabNumber(7), &s).is_some());
        s.tabs.clear();
        assert!(request(Command::TabNumber(7), &s).is_none());
    }

    #[test]
    fn creation_uses_daemon_cwd_and_explicit_targets() {
        let s = snapshot();
        assert_eq!(
            request(Command::Workspace, &s).unwrap(),
            (
                Method::WorkspaceCreate,
                json!({"source_workspace_id": s.focused_workspace_id, "focus": true})
            )
        );
        assert_eq!(
            request(Command::Tab, &s).unwrap(),
            (
                Method::TabCreate,
                json!({"workspace_id": s.focused_workspace_id, "focus": true})
            )
        );
        for (command, direction) in [(Command::SplitRight, "right"), (Command::SplitDown, "down")] {
            assert_eq!(
                request(command, &s).unwrap(),
                (
                    Method::PaneSplit,
                    json!({"target_pane_id": s.focused_pane_id, "direction": direction, "focus": true})
                )
            );
        }
    }

    #[test]
    fn empty_session_can_create_workspace_only() {
        let mut s = snapshot();
        s.focused_workspace_id = None;
        s.focused_tab_id = None;
        s.focused_pane_id = None;
        s.tabs.clear();
        assert_eq!(
            request(Command::Workspace, &s).unwrap().1,
            json!({"focus": true})
        );
        for info in COMMANDS
            .iter()
            .filter(|info| info.command != Command::Workspace)
        {
            assert!(request(info.command, &s).is_none(), "{:?}", info.command);
        }
    }

    #[test]
    fn tab_cycle_ignores_missing_or_foreign_focus() {
        let mut s = snapshot();
        let mut tab = s.tabs[0].clone();
        tab.tab_id = "foreign-tab".into();
        tab.workspace_id = "other-workspace".into();
        s.tabs.push(tab);
        for focus in [None, Some("removed-tab"), Some("foreign-tab")] {
            s.focused_tab_id = focus.map(str::to_owned);
            for command in [Command::NextTab, Command::PreviousTab] {
                assert!(request(command, &s).is_none());
            }
        }
        s.tabs.clear();
        for command in [Command::NextTab, Command::PreviousTab] {
            assert!(request(command, &s).is_none());
        }
    }

    #[test]
    fn tab_cycle_wraps_and_stays_in_workspace() {
        let mut s = snapshot();
        let mut tab = s.tabs[0].clone();
        tab.workspace_id = s.focused_workspace_id.clone().unwrap();
        tab.tab_id = "first".into();
        let mut second = tab.clone();
        second.tab_id = "second".into();
        let mut other = tab.clone();
        other.workspace_id = "other".into();
        s.tabs = vec![tab, other, second];
        s.focused_tab_id = Some("first".into());
        for command in [Command::NextTab, Command::PreviousTab] {
            assert_eq!(request(command, &s).unwrap().1, json!({"tab_id": "second"}));
        }
        s.focused_tab_id = Some("second".into());
        assert_eq!(
            request(Command::NextTab, &s).unwrap().1,
            json!({"tab_id": "first"})
        );
        s.tabs.truncate(1);
        s.focused_tab_id = Some("first".into());
        assert_eq!(
            request(Command::PreviousTab, &s).unwrap().1,
            json!({"tab_id": "first"})
        );
    }
}
