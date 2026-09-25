//! The command palette and Go To, as a kit `Command` in a dialog. Entries are
//! prepared when it opens and filtered here, so nesting follows the rows a
//! search keeps.

use crate::{
    Error, HerdrWindow, NavigationTarget, OwnedNavigationTarget, Result,
    controls::{COMMANDS, Command},
    menu::{Page, error_alert},
};
use gpui_kit::{
    component::{
        ActiveTheme as _,
        command::{Command as CommandPalette, CommandItem, CommandState},
        h_flex,
        tag::Tag,
        v_flex,
    },
    prelude::*,
    *,
};
use herdr_client::{
    Method,
    protocol::{AgentStatus, ClientShellCommandAction, ClientShellSnapshot},
};
use serde_json::{Value, json};

#[derive(Clone)]
enum Action {
    Native(Command),
    /// A Go To destination, qualified by the host and daemon boot it was listed from.
    Go {
        endpoint: String,
        boot: String,
        target: OwnedNavigationTarget,
    },
    Configured(String, ClientShellCommandAction),
}

#[derive(Clone)]
struct Entry {
    label: String,
    detail: String,
    badge: &'static str,
    action: Action,
    /// Index of the row this one nests under, indented only while that row
    /// is visible so a search never leaves it hanging beneath nothing.
    parent: Option<usize>,
}

#[derive(Clone)]
struct Target {
    boot: String,
    workspace: Option<String>,
    tab: Option<String>,
    pane: Option<String>,
}

impl Target {
    fn capture(snapshot: &ClientShellSnapshot) -> Self {
        Self {
            boot: snapshot.boot_id.clone(),
            workspace: snapshot.focused_workspace_id.clone(),
            tab: snapshot.focused_tab_id.clone(),
            pane: snapshot.focused_pane_id.clone(),
        }
    }

    fn validate_boot(&self, snapshot: &ClientShellSnapshot) -> Result<()> {
        if self.boot.is_empty() || self.boot != snapshot.boot_id {
            return Err(Error::PaletteSessionChanged);
        }
        Ok(())
    }

    fn workspace_exists(&self, snapshot: &ClientShellSnapshot, id: &str) -> Result<()> {
        self.validate_boot(snapshot)?;
        if !snapshot.workspaces.iter().any(|w| w.workspace_id == id) {
            return Err(Error::PaletteWorkspaceRemoved);
        }
        Ok(())
    }

    fn invocation(
        &self,
        snapshot: &ClientShellSnapshot,
        id: &str,
        action: ClientShellCommandAction,
    ) -> Result<Value> {
        self.validate_boot(snapshot)?;
        if action == ClientShellCommandAction::Unknown {
            return Err(Error::UnsupportedCommand);
        }
        if !snapshot
            .commands
            .iter()
            .any(|c| c.command_id == id && c.action == action)
        {
            return Err(Error::PaletteCommandChanged);
        }
        if let Some(id) = &self.workspace {
            self.workspace_exists(snapshot, id)?;
        }
        if let Some(id) = &self.tab
            && !snapshot
                .tabs
                .iter()
                .any(|t| t.tab_id == *id && self.workspace.as_ref() == Some(&t.workspace_id))
        {
            return Err(Error::PaletteTabRemoved);
        }
        if let Some(id) = &self.pane
            && !snapshot.panes.iter().any(|p| {
                p.pane_id == *id
                    && self.workspace.as_ref() == Some(&p.workspace_id)
                    && self.tab.as_ref() == Some(&p.tab_id)
            })
        {
            return Err(Error::PalettePaneRemoved);
        }
        let mut params = json!({"command_id": id});
        for (key, value) in [
            ("workspace_id", &self.workspace),
            ("tab_id", &self.tab),
            ("pane_id", &self.pane),
        ] {
            if let Some(value) = value {
                params[key] = json!(value);
            }
        }
        Ok(params)
    }
}

/// Whether a Go To destination listed from `boot` still exists in `snapshot`.
fn destination_exists(
    snapshot: &ClientShellSnapshot,
    boot: &str,
    target: NavigationTarget<&str>,
) -> Result<()> {
    if boot.is_empty() || boot != snapshot.boot_id {
        return Err(Error::PaletteSessionChanged);
    }
    match target {
        NavigationTarget::Workspace(id) => snapshot
            .workspaces
            .iter()
            .any(|w| w.workspace_id == id)
            .then_some(())
            .ok_or(Error::PaletteWorkspaceRemoved),
        NavigationTarget::Tab(id) => snapshot
            .tabs
            .iter()
            .any(|t| t.tab_id == id)
            .then_some(())
            .ok_or(Error::PaletteTabRemoved),
        NavigationTarget::Pane(id) => snapshot
            .panes
            .iter()
            .any(|p| p.pane_id == id)
            .then_some(())
            .ok_or(Error::PaletteDestinationRemoved),
    }
}

fn status_badge(status: AgentStatus) -> &'static str {
    match status {
        AgentStatus::Blocked => "blocked",
        AgentStatus::Done => "done",
        AgentStatus::Working => "working",
        AgentStatus::Idle => "idle",
        AgentStatus::Unknown => "",
    }
}

/// One host's Go To rows: each workspace, then every pane in it, one row per
/// agent or terminal so no split is hidden behind its tab.
fn go_to_entries(
    endpoint: &str,
    host: Option<&str>,
    snapshot: &ClientShellSnapshot,
    entries: &mut Vec<Entry>,
) {
    let go = |target| Action::Go {
        endpoint: endpoint.to_owned(),
        boot: snapshot.boot_id.clone(),
        target,
    };
    for workspace in &snapshot.workspaces {
        let detail = [
            host,
            Some(&*format!("#{}", workspace.number)),
            workspace.branch.as_deref(),
        ]
        .into_iter()
        .flatten()
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("  ");
        let parent = entries.len();
        entries.push(Entry {
            label: workspace.label.clone(),
            detail,
            badge: "",
            action: go(NavigationTarget::Workspace(workspace.workspace_id.clone())),
            parent: None,
        });
        let tabs: Vec<_> = snapshot
            .tabs
            .iter()
            .filter(|tab| tab.workspace_id == workspace.workspace_id)
            .collect();
        let panes: Vec<_> = tabs
            .iter()
            .flat_map(|tab| {
                snapshot
                    .panes
                    .iter()
                    .filter(move |pane| {
                        pane.workspace_id == workspace.workspace_id && pane.tab_id == tab.tab_id
                    })
                    .map(move |pane| (*tab, pane))
            })
            .collect();
        for (tab, pane) in &panes {
            let agent = snapshot
                .agents
                .iter()
                .find(|agent| agent.pane_id == pane.pane_id);
            let name = match agent {
                Some(agent) => crate::sidebar::agent_name(agent),
                None => pane
                    .label
                    .as_deref()
                    .map(str::trim)
                    .filter(|label| !label.is_empty())
                    .unwrap_or("Terminal"),
            };
            // As in the sidebar, the tab only earns its place when there is a choice.
            let tab = (tabs.len() > 1 || tab.custom_label).then_some(tab.label.as_str());
            let path = pane.foreground_cwd.as_deref().or(pane.cwd.as_deref());
            let detail = [host, tab, path]
                .into_iter()
                .flatten()
                .filter(|text| !text.is_empty())
                .collect::<Vec<_>>()
                .join("  ");
            entries.push(Entry {
                label: name.to_owned(),
                detail,
                badge: agent.map_or("", |agent| status_badge(agent.agent_status)),
                action: go(NavigationTarget::Pane(pane.pane_id.clone())),
                parent: Some(parent),
            });
        }
    }
}

fn matches_query(text: &str, query: &str) -> bool {
    let text = text.to_lowercase();
    query
        .split_whitespace()
        .all(|token| text.contains(&token.to_lowercase()))
}

/// Whether a row sits under its parent in the visible list. `filtered` keeps
/// entry order, so it stays sorted.
fn is_nested(entry: &Entry, filtered: &[usize]) -> bool {
    entry
        .parent
        .is_some_and(|parent| filtered.binary_search(&parent).is_ok())
}

pub(crate) struct Palette {
    pub(crate) command: Entity<CommandState>,
    entries: Vec<Entry>,
    filtered: Vec<usize>,
    target: Option<Target>,
    workspaces_only: bool,
    error: Option<String>,
}

impl Palette {
    fn filter(&mut self, query: &str) {
        self.filtered = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                let id = match &entry.action {
                    Action::Configured(id, _) => id.as_str(),
                    Action::Native(_) | Action::Go { .. } => "",
                };
                matches_query(
                    &format!("{} {} {} {id}", entry.label, entry.detail, entry.badge),
                    query,
                )
                .then_some(index)
            })
            .collect();
    }

    /// One kit command row: label and detail, indented beneath a visible
    /// parent, with the status badge trailing.
    fn item(&self, index: usize) -> CommandItem {
        let entry = &self.entries[index];
        let nested = is_nested(entry, &self.filtered);
        let (label, detail, badge) = (
            SharedString::from(entry.label.clone()),
            SharedString::from(entry.detail.clone()),
            entry.badge,
        );
        CommandItem::new().label(label.clone()).child(move |_, cx| {
            h_flex()
                .w_full()
                .gap_3()
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .when(nested, |column| column.pl_4())
                        .child(div().truncate().child(label.clone()))
                        .when(!detail.is_empty(), |column| {
                            column.child(
                                div()
                                    .truncate()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child(detail.clone()),
                            )
                        }),
                )
                .when(!badge.is_empty(), |row| {
                    row.child(Tag::secondary().flex_none().child(badge))
                })
        })
    }
}

impl HerdrWindow {
    pub(crate) fn open_palette(
        &mut self,
        workspaces_only: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.begin_menu(window, cx) {
            return;
        }
        let mut entries = Vec::new();
        if !workspaces_only {
            entries.extend(
                COMMANDS
                    .iter()
                    .filter(|info| info.command != Command::Palette)
                    .filter(|info| {
                        info.command != Command::ClearPane || self.live.supports_pane_clear
                    })
                    .map(|info| Entry {
                        label: info.label.into(),
                        detail: self.config.keybindings.primary(info.command).into(),
                        badge: "",
                        action: Action::Native(info.command),
                        parent: None,
                    }),
            );
        }
        if workspaces_only {
            // The selected host first, so Enter on an unfiltered list stays local
            // to what is on screen; the other connected hosts follow in order.
            let selected = self.selected_endpoint;
            let order = std::iter::once(selected)
                .chain((0..self.endpoints.len()).filter(|index| *index != selected));
            for index in order {
                let endpoint = &self.endpoints[index];
                let live = if index == selected {
                    &self.live
                } else {
                    &endpoint.live
                };
                let Some(snapshot) = live.snapshot.as_ref().filter(|_| endpoint.enabled) else {
                    continue;
                };
                let host =
                    (endpoint.id != crate::endpoint::LOCAL).then_some(endpoint.label.as_str());
                go_to_entries(&endpoint.id, host, snapshot, &mut entries);
            }
        }
        let target = self.live.snapshot.as_ref().map(|snapshot| {
            if !workspaces_only {
                entries.extend(snapshot.commands.iter().map(|command| {
                    let mut bindings = command.binding_labels.clone();
                    if !command.binding_label.is_empty()
                        && !bindings.contains(&command.binding_label)
                    {
                        bindings.push(command.binding_label.clone());
                    }
                    bindings.retain(|binding| !binding.is_empty());
                    Entry {
                        label: command
                            .description
                            .as_ref()
                            .filter(|s| !s.trim().is_empty())
                            .unwrap_or(&command.command_id)
                            .clone(),
                        detail: if bindings.is_empty() {
                            String::new()
                        } else {
                            format!(
                                "Daemon bindings: {} (not GUI shortcuts)",
                                bindings.join(", ")
                            )
                        },
                        badge: "Herdr command",
                        action: Action::Configured(command.command_id.clone(), command.action),
                        parent: None,
                    }
                }));
            }
            Target::capture(snapshot)
        });
        let command = cx.new(|cx| CommandState::new(window, cx));
        let mut palette = Palette {
            command: command.clone(),
            entries,
            filtered: Vec::new(),
            target,
            workspaces_only,
            error: None,
        };
        palette.filter("");
        self.menu.palette = Some(palette);
        self.show_dialog(Page::Palette, window, cx, |this, dialog, weak, _, cx| {
            let Some(palette) = &this.menu.palette else {
                return dialog;
            };
            let query = weak.clone();
            let confirm = weak.clone();
            dialog
                .title(if palette.workspaces_only {
                    "Go To"
                } else {
                    "Command Palette"
                })
                .w(px(640.))
                .child(
                    v_flex()
                        .gap_2()
                        .children(error_alert("palette-error", palette.error.as_ref()))
                        .child(
                            CommandPalette::new(&palette.command)
                                .bordered(false)
                                .filterable(false)
                                .max_h(px(420.))
                                .placeholder(if palette.workspaces_only {
                                    "Search workspaces, agents, and terminals..."
                                } else {
                                    "Search commands..."
                                })
                                .items(palette.filtered.iter().map(|index| palette.item(*index)))
                                .empty(|_, _, _| "No matching results. Try a shorter search.")
                                .on_query(move |text, _, cx| {
                                    let _ = query.update(cx, |this, cx| {
                                        if let Some(palette) = &mut this.menu.palette {
                                            palette.filter(text);
                                            cx.notify();
                                        }
                                    });
                                })
                                .on_confirm(move |index, window, cx| {
                                    let _ = confirm.update(cx, |this, cx| {
                                        let action =
                                            this.menu.palette.as_ref().and_then(|palette| {
                                                let entry = palette.filtered.get(index.row)?;
                                                Some(palette.entries[*entry].action.clone())
                                            });
                                        if let Some(action) = action {
                                            this.activate_palette(action, window, cx);
                                        }
                                    });
                                }),
                        )
                        .child(
                            div()
                                .debug_selector(|| "palette-status".into())
                                .text_sm()
                                .text_color(cx.theme().muted_foreground)
                                .child(format!(
                                    "{} of {} results",
                                    palette.filtered.len(),
                                    palette.entries.len()
                                )),
                        ),
                )
        });
        command.update(cx, |command, cx| command.focus(window, cx));
    }

    fn activate_palette(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        if !self.menu_target_current() {
            if let Some(palette) = &mut self.menu.palette {
                palette.error = Some("The selected connection changed. Reopen the palette.".into());
            }
            cx.notify();
            return;
        }
        if let Action::Native(command) = action {
            self.dismiss_menu(window, cx);
            self.command(command, window, cx);
            return;
        }
        if let Action::Go {
            endpoint,
            boot,
            target,
        } = &action
        {
            match self.go_to_ready(endpoint, boot, target.as_deref()) {
                Ok(selected) => {
                    self.dismiss_menu(window, cx);
                    if selected {
                        self.navigate(target.as_deref(), cx);
                    } else {
                        self.navigate_endpoint(endpoint, target.as_deref(), cx);
                    }
                }
                Err(error) => {
                    if let Some(palette) = &mut self.menu.palette {
                        palette.error = Some(error.to_string());
                    }
                    cx.notify();
                }
            }
            return;
        }
        let result = (|| {
            if !self.input_ready() {
                return Err(Error::PaletteConnectionNotReady);
            }
            let snapshot = self.live.snapshot.as_ref().ok_or(Error::NoSnapshot)?;
            let target = self
                .menu
                .palette
                .as_ref()
                .and_then(|p| p.target.as_ref())
                .ok_or(Error::NoPaletteSession)?;
            match &action {
                Action::Configured(id, action) => target.invocation(snapshot, id, *action),
                Action::Native(_) | Action::Go { .. } => unreachable!(),
            }
        })();
        match result {
            Ok(params) => {
                self.request_focus_change(Method::CommandInvoke.as_str(), None, |handle, boot| {
                    handle.request(boot, Method::CommandInvoke, params)
                });
                self.dismiss_menu(window, cx);
            }
            Err(error) => {
                if let Some(palette) = &mut self.menu.palette {
                    palette.error = Some(error.to_string());
                }
                cx.notify();
            }
        }
    }

    /// Confirms the palette's `row`th visible entry, as Enter on it would.
    #[cfg(all(test, unix))]
    pub(crate) fn confirm_palette_row(
        &mut self,
        row: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let action = self.menu.palette.as_ref().and_then(|palette| {
            let entry = palette.filtered.get(row)?;
            Some(palette.entries[*entry].action.clone())
        });
        if let Some(action) = action {
            self.activate_palette(action, window, cx);
        }
    }

    /// Checks a Go To destination against its host's current snapshot, and
    /// reports whether that host is the selected one. Another host is selected
    /// by the navigation itself, which waits for its surface when needed.
    fn go_to_ready(
        &self,
        endpoint: &str,
        boot: &str,
        target: NavigationTarget<&str>,
    ) -> Result<bool> {
        let index = self
            .endpoints
            .iter()
            .position(|e| e.id == endpoint && e.enabled)
            .ok_or(Error::PaletteHostUnavailable)?;
        let selected = index == self.selected_endpoint;
        let live = if selected {
            &self.live
        } else {
            &self.endpoints[index].live
        };
        let snapshot = live
            .snapshot
            .as_ref()
            .ok_or(Error::PaletteHostUnavailable)?;
        destination_exists(snapshot, boot, target)?;
        if selected && !self.input_ready() {
            return Err(Error::PaletteConnectionNotReady);
        }
        Ok(selected)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    use gpui_kit::TestAppContext;
    use herdr_client::protocol::ClientShellCommand;

    fn snapshot() -> ClientShellSnapshot {
        serde_json::from_str(include_str!(
            "../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
        ))
        .unwrap()
    }

    #[gpui_kit::test]
    fn notification_command_is_searchable_and_targetless_activation_is_inert(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) =
            crate::test_support::add_window_view(cx, crate::sidebar::layout_tests::fixture_window);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.show_toast_preview(
                    herdr_client::protocol::SemanticNotificationKind::Custom,
                    cx,
                );
                let selected = view.selected_endpoint;
                view.open_palette(false, window, cx);
                let palette = view.menu.palette.as_mut().unwrap();
                palette.filter("Open Notification Target");
                assert_eq!(palette.filtered.len(), 1);
                let action = palette.entries[palette.filtered[0]].action.clone();
                assert!(matches!(
                    action,
                    Action::Native(Command::OpenNotificationTarget)
                ));
                view.activate_palette(action, window, cx);
                assert!(view.menu.page.is_none());
                assert!(view.pending_navigation.is_none());
                assert_eq!(view.selected_endpoint, selected);
                assert_eq!(view.endpoints[selected].toasts.entries.len(), 1);
            })
        });
        cx.update(|window, cx| {
            crate::bind_keys(cx);
            let focus = view.read(cx).focus.clone();
            window.focus(&focus, cx);
            window.draw(cx).clear(cx);
            window.dispatch_keystroke(Keystroke::parse("cmd-alt-n").unwrap(), cx);
            assert!(view.read(cx).pending_navigation.is_none());
            assert_eq!(view.read(cx).endpoints[0].toasts.entries.len(), 1);
        });
    }

    #[gpui_kit::test]
    fn command_badges_mark_daemon_commands_and_go_to_badges_mark_agent_status(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) =
            crate::test_support::add_window_view(cx, crate::sidebar::layout_tests::fixture_window);
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                let snapshot = std::sync::Arc::make_mut(view.live.snapshot.as_mut().unwrap());
                snapshot.commands = vec![ClientShellCommand {
                    command_id: "build".into(),
                    action: ClientShellCommandAction::Shell,
                    description: None,
                    binding_label: String::new(),
                    binding_labels: Vec::new(),
                }];
            })
        });
        for workspaces_only in [true, false] {
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.open_palette(workspaces_only, window, cx)
                })
            });
            cx.run_until_parked();
            view.read_with(cx, |view, _| {
                let entries = &view.menu.palette.as_ref().unwrap().entries;
                assert!(!entries.is_empty());
                for entry in entries {
                    let expected = match entry.action {
                        Action::Native(_) => "",
                        Action::Configured(..) => "Herdr command",
                        Action::Go {
                            target: NavigationTarget::Pane(_),
                            ..
                        } => "blocked",
                        Action::Go { .. } => "",
                    };
                    assert_eq!(entry.badge, expected, "{}", entry.label);
                }
                assert_eq!(
                    entries
                        .iter()
                        .any(|entry| matches!(entry.action, Action::Go { .. })),
                    workspaces_only
                );
            });
            cx.update(|window, cx| view.update(cx, |view, cx| view.dismiss_menu(window, cx)));
        }
    }

    #[gpui_kit::test]
    fn clear_pane_is_offered_and_sent_only_when_the_daemon_advertises_it(cx: &mut TestAppContext) {
        let (view, cx) =
            crate::test_support::add_window_view(cx, crate::sidebar::layout_tests::fixture_window);
        for supported in [false, true] {
            cx.update(|window, cx| {
                view.update(cx, |view, cx| {
                    view.live.supports_pane_clear = supported;
                    view.open_palette(false, window, cx);
                    let offered = view
                        .menu
                        .palette
                        .as_ref()
                        .unwrap()
                        .entries
                        .iter()
                        .any(|entry| matches!(entry.action, Action::Native(Command::ClearPane)));
                    assert_eq!(offered, supported);
                    view.dismiss_menu(window, cx);
                })
            });
        }
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.live.supports_pane_clear = false;
                view.local_error = None;
                view.command(Command::ClearPane, window, cx);
                assert!(
                    view.local_error
                        .as_deref()
                        .is_some_and(|error| error.contains("newer Herdr"))
                );
                assert!(view.activation_deadline.is_none(), "nothing was sent");
            })
        });
    }

    #[gpui_kit::test]
    fn go_to_rejects_a_disabled_or_missing_host_and_keeps_the_palette_open(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) =
            crate::test_support::add_window_view(cx, crate::sidebar::layout_tests::fixture_window);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_palette(true, window, cx);
                let boot = view.live.snapshot.as_ref().unwrap().boot_id.clone();
                for endpoint in ["missing-host", crate::endpoint::LOCAL] {
                    if endpoint == crate::endpoint::LOCAL {
                        view.endpoints[0].enabled = false;
                    }
                    view.activate_palette(
                        Action::Go {
                            endpoint: endpoint.into(),
                            boot: boot.clone(),
                            target: NavigationTarget::Pane("w1:p1".into()),
                        },
                        window,
                        cx,
                    );
                    let palette = view.menu.palette.as_ref().unwrap();
                    assert_eq!(
                        palette.error.as_deref(),
                        Some(Error::PaletteHostUnavailable.to_string().as_str())
                    );
                    assert!(view.pending_navigation.is_none());
                }
            })
        });
    }

    #[gpui_kit::test]
    fn go_to_lists_the_selected_host_first_and_switches_host_for_a_remote_row(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) = crate::test_support::add_window_view(cx, |window, cx| {
            let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
            let mut remote = crate::endpoint::Endpoint::new(
                "ssh:box".into(),
                "Box".into(),
                herdr_client::ConnectTarget::Ssh {
                    target: "unused".into(),
                    session: "default".into(),
                },
                true,
            );
            remote.live.snapshot = Some(std::sync::Arc::new(snapshot()));
            view.endpoints.push(remote);
            view
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.open_palette(true, window, cx);
                let entries = &view.menu.palette.as_ref().unwrap().entries;
                let hosts: Vec<_> = entries
                    .iter()
                    .map(|entry| match &entry.action {
                        Action::Go { endpoint, .. } => endpoint.as_str(),
                        _ => panic!("Go To lists only destinations"),
                    })
                    .collect();
                let local = hosts
                    .iter()
                    .take_while(|host| **host == crate::endpoint::LOCAL)
                    .count();
                assert!(local > 0 && local < hosts.len());
                assert!(hosts[local..].iter().all(|host| *host == "ssh:box"));
                assert!(entries[..local].iter().all(|e| !e.detail.contains("Box")));
                assert!(entries[local..].iter().all(|e| e.detail.starts_with("Box")));
                let remote_pane = entries[local..]
                    .iter()
                    .find(|entry| {
                        matches!(
                            entry.action,
                            Action::Go {
                                target: NavigationTarget::Pane(_),
                                ..
                            }
                        )
                    })
                    .unwrap()
                    .action
                    .clone();
                let Action::Go { target, .. } = &remote_pane else {
                    unreachable!()
                };
                let target = target.clone();
                view.activate_palette(remote_pane, window, cx);
                assert!(view.menu.page.is_none(), "a valid row closes the picker");
                assert_eq!(view.endpoints[view.selected_endpoint].id, "ssh:box");
                // The remote host has no connection yet, so navigation waits for it.
                assert_eq!(view.pending_navigation, Some(target));
            })
        });
    }

    fn go_to_fixture() -> ClientShellSnapshot {
        let mut snapshot = snapshot();
        let mut tab = snapshot.tabs[0].clone();
        tab.tab_id = "w1:t2".into();
        tab.label = "logs".into();
        snapshot.tabs.push(tab);
        let mut terminal = snapshot.panes[0].clone();
        terminal.pane_id = "w1:p2".into();
        terminal.tab_id = "w1:t2".into();
        terminal.label = Some("  ".into());
        terminal.foreground_cwd = None;
        terminal.cwd = Some("/repo/logs".into());
        snapshot.panes.push(terminal);
        let mut empty = snapshot.workspaces[0].clone();
        empty.workspace_id = "w2".into();
        empty.number = 2;
        empty.label = "empty".into();
        empty.branch = None;
        snapshot.workspaces.push(empty);
        snapshot
    }

    #[test]
    fn go_to_lists_every_pane_under_its_workspace() {
        let snapshot = go_to_fixture();
        let mut entries = Vec::new();
        go_to_entries(crate::endpoint::LOCAL, None, &snapshot, &mut entries);
        let rows: Vec<_> = entries
            .iter()
            .map(|entry| {
                let Action::Go {
                    endpoint,
                    boot,
                    target,
                } = &entry.action
                else {
                    panic!("Go To lists only destinations");
                };
                assert_eq!(endpoint, crate::endpoint::LOCAL);
                assert_eq!(boot, "boot-v1");
                (
                    entry.label.as_str(),
                    entry.detail.as_str(),
                    entry.badge,
                    target.clone(),
                )
            })
            .collect();
        assert_eq!(
            rows,
            [
                (
                    "repo",
                    "#1  main",
                    "",
                    NavigationTarget::Workspace("w1".into())
                ),
                (
                    "Claude",
                    "main  /repo",
                    "blocked",
                    NavigationTarget::Pane("w1:p1".into())
                ),
                (
                    "Terminal",
                    "logs  /repo/logs",
                    "",
                    NavigationTarget::Pane("w1:p2".into())
                ),
                ("empty", "#2", "", NavigationTarget::Workspace("w2".into())),
            ]
        );
    }

    #[test]
    fn go_to_names_remote_hosts_and_hides_a_lone_default_tab() {
        let snapshot = snapshot();
        let mut entries = Vec::new();
        go_to_entries("ssh:box", Some("Box"), &snapshot, &mut entries);
        let details: Vec<_> = entries.iter().map(|entry| entry.detail.as_str()).collect();
        assert_eq!(details, ["Box  #1  main", "Box  /repo"]);
        assert!(entries.iter().all(|entry| matches!(
            &entry.action,
            Action::Go { endpoint, .. } if endpoint == "ssh:box"
        )));
        let mut matching = entries
            .iter()
            .filter(|entry| {
                matches_query(
                    &format!("{} {} {}", entry.label, entry.detail, entry.badge),
                    "box claude",
                )
            })
            .map(|entry| entry.label.as_str());
        assert_eq!(matching.next(), Some("Claude"));
        assert_eq!(matching.next(), None);
    }

    #[test]
    fn go_to_panes_indent_only_beneath_a_visible_workspace() {
        let snapshot = go_to_fixture();
        let mut entries = Vec::new();
        go_to_entries(crate::endpoint::LOCAL, None, &snapshot, &mut entries);
        let nested = |filtered: &[usize]| {
            filtered
                .iter()
                .map(|index| is_nested(&entries[*index], filtered))
                .collect::<Vec<_>>()
        };
        assert_eq!(nested(&[0, 1, 2, 3]), [false, true, true, false]);
        // A search that matches a pane but not its workspace leaves it flush.
        assert_eq!(nested(&[1, 3]), [false, false]);
    }

    #[test]
    fn go_to_destinations_are_revalidated_against_the_current_snapshot() {
        let snapshot = go_to_fixture();
        for target in [
            NavigationTarget::Workspace("w2"),
            NavigationTarget::Tab("w1:t2"),
            NavigationTarget::Pane("w1:p2"),
        ] {
            assert!(destination_exists(&snapshot, "boot-v1", target.clone()).is_ok());
            assert!(matches!(
                destination_exists(&snapshot, "boot-v2", target.clone()),
                Err(Error::PaletteSessionChanged)
            ));
            assert!(matches!(
                destination_exists(&snapshot, "", target),
                Err(Error::PaletteSessionChanged)
            ));
        }
        assert!(matches!(
            destination_exists(&snapshot, "boot-v1", NavigationTarget::Workspace("gone")),
            Err(Error::PaletteWorkspaceRemoved)
        ));
        assert!(matches!(
            destination_exists(&snapshot, "boot-v1", NavigationTarget::Tab("gone")),
            Err(Error::PaletteTabRemoved)
        ));
        assert!(matches!(
            destination_exists(&snapshot, "boot-v1", NavigationTarget::Pane("gone")),
            Err(Error::PaletteDestinationRemoved)
        ));
    }

    #[test]
    fn filtering_matches_all_unicode_tokens_in_any_order() {
        assert!(matches_query("CAF\u{c9} branch 42", " 42\tCAF\u{e9} "));
        assert!(matches_query("\u{391}\u{392} workspace", "\u{3b1}\u{3b2}"));
        assert!(matches_query("anything", " \n "));
        assert!(!matches_query("CAF\u{c9} branch 42", "caf\u{e9} missing"));
    }

    #[test]
    fn invocation_uses_captured_ids_not_current_focus() {
        let mut snapshot = snapshot();
        snapshot.commands = vec![ClientShellCommand {
            command_id: "build".into(),
            action: ClientShellCommandAction::Shell,
            description: None,
            binding_label: String::new(),
            binding_labels: Vec::new(),
        }];
        let target = Target::capture(&snapshot);
        snapshot.focused_workspace_id = None;
        snapshot.focused_tab_id = None;
        snapshot.focused_pane_id = None;
        assert_eq!(
            target
                .invocation(&snapshot, "build", ClientShellCommandAction::Shell)
                .unwrap(),
            json!({
                "command_id": "build", "workspace_id": target.workspace,
                "tab_id": target.tab, "pane_id": target.pane,
            })
        );
        let empty_target = Target::capture(&snapshot);
        assert_eq!(
            empty_target
                .invocation(&snapshot, "build", ClientShellCommandAction::Shell)
                .unwrap(),
            json!({"command_id": "build"})
        );
    }

    #[test]
    fn invocation_rejects_stale_boot_command_action_and_membership() {
        let mut original = snapshot();
        original.commands = vec![ClientShellCommand {
            command_id: "build".into(),
            action: ClientShellCommandAction::Shell,
            description: None,
            binding_label: String::new(),
            binding_labels: Vec::new(),
        }];
        let target = Target::capture(&original);
        for change in 0..7 {
            let mut snapshot = original.clone();
            match change {
                0 => snapshot.boot_id.push_str("-new"),
                1 => snapshot.commands.clear(),
                2 => snapshot.commands[0].action = ClientShellCommandAction::Pane,
                3 => snapshot.workspaces.clear(),
                4 => snapshot.tabs.clear(),
                5 => snapshot.panes.clear(),
                _ => {
                    for pane in &mut snapshot.panes {
                        pane.tab_id = "foreign".into();
                    }
                }
            }
            assert!(
                target
                    .invocation(&snapshot, "build", ClientShellCommandAction::Shell)
                    .is_err()
            );
        }
        original.commands[0].action = ClientShellCommandAction::Unknown;
        assert!(
            target
                .invocation(&original, "build", ClientShellCommandAction::Unknown)
                .is_err()
        );
    }

    #[test]
    fn workspace_selection_rejects_removed_id_and_restarted_daemon() {
        let mut snapshot = snapshot();
        let target = Target::capture(&snapshot);
        let id = snapshot.workspaces[0].workspace_id.clone();
        assert!(target.workspace_exists(&snapshot, &id).is_ok());
        assert!(target.workspace_exists(&snapshot, "missing").is_err());
        snapshot.boot_id.push_str("-new");
        assert!(target.workspace_exists(&snapshot, &id).is_err());
    }
}
