use herdr_client::protocol::ClientShellSnapshot;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
pub enum Command {
    Workspace,
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
    ClosePane,
    CloseTab,
    TabNumber(u8),
    ToggleSidebar,
    Settings,
    Keybinds,
    Themes,
    WorkspacePicker,
    Palette,
    Reconnect,
    Quit,
}

pub struct CommandInfo {
    pub command: Command,
    pub label: &'static str,
    pub shortcut: &'static str,
}

pub const COMMANDS: &[CommandInfo] = &[
    CommandInfo {
        command: Command::Workspace,
        label: "New Workspace",
        shortcut: "cmd-n",
    },
    CommandInfo {
        command: Command::Tab,
        label: "New Tab",
        shortcut: "cmd-t",
    },
    CommandInfo {
        command: Command::SplitRight,
        label: "Split Right",
        shortcut: "cmd-d",
    },
    CommandInfo {
        command: Command::SplitDown,
        label: "Split Down",
        shortcut: "cmd-shift-d",
    },
    CommandInfo {
        command: Command::NextTab,
        label: "Next Tab",
        shortcut: "cmd-shift-]",
    },
    CommandInfo {
        command: Command::PreviousTab,
        label: "Previous Tab",
        shortcut: "cmd-shift-[",
    },
    CommandInfo {
        command: Command::FocusLeft,
        label: "Focus Left",
        shortcut: "cmd-alt-left",
    },
    CommandInfo {
        command: Command::FocusRight,
        label: "Focus Right",
        shortcut: "cmd-alt-right",
    },
    CommandInfo {
        command: Command::FocusUp,
        label: "Focus Up",
        shortcut: "cmd-alt-up",
    },
    CommandInfo {
        command: Command::FocusDown,
        label: "Focus Down",
        shortcut: "cmd-alt-down",
    },
    CommandInfo {
        command: Command::NextPane,
        label: "Next Pane",
        shortcut: "cmd-alt-]",
    },
    CommandInfo {
        command: Command::PreviousPane,
        label: "Previous Pane",
        shortcut: "cmd-alt-[",
    },
    CommandInfo {
        command: Command::Zoom,
        label: "Toggle Pane Zoom",
        shortcut: "cmd-shift-enter",
    },
    CommandInfo {
        command: Command::ClosePane,
        label: "Close Pane",
        shortcut: "cmd-w",
    },
    CommandInfo {
        command: Command::CloseTab,
        label: "Close Tab",
        shortcut: "cmd-shift-w",
    },
    CommandInfo {
        command: Command::TabNumber(1),
        label: "Focus Tab 1",
        shortcut: "cmd-1",
    },
    CommandInfo {
        command: Command::TabNumber(2),
        label: "Focus Tab 2",
        shortcut: "cmd-2",
    },
    CommandInfo {
        command: Command::TabNumber(3),
        label: "Focus Tab 3",
        shortcut: "cmd-3",
    },
    CommandInfo {
        command: Command::TabNumber(4),
        label: "Focus Tab 4",
        shortcut: "cmd-4",
    },
    CommandInfo {
        command: Command::TabNumber(5),
        label: "Focus Tab 5",
        shortcut: "cmd-5",
    },
    CommandInfo {
        command: Command::TabNumber(6),
        label: "Focus Tab 6",
        shortcut: "cmd-6",
    },
    CommandInfo {
        command: Command::TabNumber(7),
        label: "Focus Tab 7",
        shortcut: "cmd-7",
    },
    CommandInfo {
        command: Command::TabNumber(8),
        label: "Focus Tab 8",
        shortcut: "cmd-8",
    },
    CommandInfo {
        command: Command::TabNumber(9),
        label: "Focus Tab 9",
        shortcut: "cmd-9",
    },
    CommandInfo {
        command: Command::ToggleSidebar,
        label: "Toggle Sidebar",
        shortcut: "cmd-b",
    },
    CommandInfo {
        command: Command::Settings,
        label: "Settings",
        shortcut: "cmd-,",
    },
    CommandInfo {
        command: Command::Keybinds,
        label: "Keybindings",
        shortcut: "cmd-/",
    },
    CommandInfo {
        command: Command::Themes,
        label: "Themes",
        shortcut: "",
    },
    CommandInfo {
        command: Command::WorkspacePicker,
        label: "Workspace Picker",
        shortcut: "cmd-p",
    },
    CommandInfo {
        command: Command::Palette,
        label: "Command Palette",
        shortcut: "cmd-shift-p",
    },
    CommandInfo {
        command: Command::Reconnect,
        label: "Reconnect",
        shortcut: "",
    },
    CommandInfo {
        command: Command::Quit,
        label: "Quit",
        shortcut: "cmd-q",
    },
];

pub fn request(command: Command, snapshot: &ClientShellSnapshot) -> Option<(&'static str, Value)> {
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
            ("workspace.create", params)
        }
        Command::Tab => (
            "tab.create",
            json!({"workspace_id": workspace?.workspace_id, "focus": true}),
        ),
        Command::SplitRight | Command::SplitDown => (
            "pane.split",
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
            ("tab.focus", json!({"tab_id": tabs[next].tab_id}))
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
                "pane.focus_direction",
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
            ("pane.focus", json!({"pane_id": panes[next].pane_id}))
        }
        Command::Zoom => (
            "pane.zoom",
            json!({"pane_id": pane?.pane_id, "mode": "toggle"}),
        ),
        Command::ClosePane => ("pane.close", json!({"pane_id": pane?.pane_id})),
        Command::CloseTab => ("tab.close", json!({"tab_id": tab?.tab_id})),
        Command::TabNumber(number) => {
            let workspace = workspace?;
            let target = snapshot.tabs.iter().find(|t| {
                t.workspace_id == workspace.workspace_id && t.number == usize::from(number)
            })?;
            ("tab.focus", json!({"tab_id": target.tab_id}))
        }
        Command::ToggleSidebar
        | Command::Settings
        | Command::Keybinds
        | Command::Themes
        | Command::WorkspacePicker
        | Command::Palette
        | Command::Reconnect
        | Command::Quit => return None,
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
        let expected = [
            (Workspace, "cmd-n"),
            (Tab, "cmd-t"),
            (SplitRight, "cmd-d"),
            (SplitDown, "cmd-shift-d"),
            (NextTab, "cmd-shift-]"),
            (PreviousTab, "cmd-shift-["),
            (FocusLeft, "cmd-alt-left"),
            (FocusRight, "cmd-alt-right"),
            (FocusUp, "cmd-alt-up"),
            (FocusDown, "cmd-alt-down"),
            (NextPane, "cmd-alt-]"),
            (PreviousPane, "cmd-alt-["),
            (Zoom, "cmd-shift-enter"),
            (ClosePane, "cmd-w"),
            (CloseTab, "cmd-shift-w"),
            (TabNumber(1), "cmd-1"),
            (TabNumber(2), "cmd-2"),
            (TabNumber(3), "cmd-3"),
            (TabNumber(4), "cmd-4"),
            (TabNumber(5), "cmd-5"),
            (TabNumber(6), "cmd-6"),
            (TabNumber(7), "cmd-7"),
            (TabNumber(8), "cmd-8"),
            (TabNumber(9), "cmd-9"),
            (ToggleSidebar, "cmd-b"),
            (Settings, "cmd-,"),
            (Keybinds, "cmd-/"),
            (Themes, ""),
            (WorkspacePicker, "cmd-p"),
            (Palette, "cmd-shift-p"),
            (Reconnect, ""),
            (Quit, "cmd-q"),
        ];
        assert_eq!(COMMANDS.len(), expected.len());
        for (info, (command, shortcut)) in COMMANDS.iter().zip(expected) {
            assert_eq!(info.command, command);
            assert_eq!(info.shortcut, shortcut);
            assert!(!info.label.is_empty());
            let value = match command {
                TabNumber(number) => json!({"TabNumber": number}),
                _ => json!(format!("{command:?}")),
            };
            assert_eq!(serde_json::from_value::<Command>(value).unwrap(), command);
        }
    }

    #[test]
    fn gui_commands_never_send_daemon_requests() {
        let s = snapshot();
        for command in [
            Command::ToggleSidebar,
            Command::Settings,
            Command::Keybinds,
            Command::Themes,
            Command::WorkspacePicker,
            Command::Palette,
            Command::Reconnect,
            Command::Quit,
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
                    "pane.focus_direction",
                    json!({"pane_id": s.focused_pane_id, "direction": direction})
                ))
            );
        }
        assert_eq!(
            request(Command::Zoom, &s),
            Some((
                "pane.zoom",
                json!({"pane_id": s.focused_pane_id, "mode": "toggle"})
            ))
        );
        assert_eq!(
            request(Command::ClosePane, &s),
            Some(("pane.close", json!({"pane_id": s.focused_pane_id})))
        );
        assert_eq!(
            request(Command::CloseTab, &s),
            Some(("tab.close", json!({"tab_id": s.focused_tab_id})))
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
                    Some(("pane.focus", json!({"pane_id": target})))
                );
            }
        }
        s.panes.truncate(1);
        s.focused_pane_id = Some(first.pane_id.clone());
        for command in [Command::NextPane, Command::PreviousPane] {
            assert_eq!(
                request(command, &s),
                Some(("pane.focus", json!({"pane_id": first.pane_id})))
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
            Some(("tab.focus", json!({"tab_id": tab.tab_id})))
        );
        assert_eq!(
            request(Command::TabNumber(2), &s),
            Some(("tab.focus", json!({"tab_id": "second"})))
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
                "workspace.create",
                json!({"source_workspace_id": s.focused_workspace_id, "focus": true})
            )
        );
        assert_eq!(
            request(Command::Tab, &s).unwrap(),
            (
                "tab.create",
                json!({"workspace_id": s.focused_workspace_id, "focus": true})
            )
        );
        for (command, direction) in [(Command::SplitRight, "right"), (Command::SplitDown, "down")] {
            assert_eq!(
                request(command, &s).unwrap(),
                (
                    "pane.split",
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
