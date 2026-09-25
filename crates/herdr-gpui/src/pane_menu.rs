//! A pane's context menu, opened by a right click in the terminal, and its
//! rename dialog. Like the tab menu, it acts on the pane that was clicked.

use crate::{
    HerdrWindow,
    close_modal::CloseConfirmation,
    menu::{Page, dialog_buttons, error_alert, listener, submit},
};
use gpui_kit::{
    component::{
        Icon, IconName,
        button::{Button, ButtonVariants as _},
        input::Input,
        menu::PopupMenuItem,
        v_flex,
    },
    prelude::*,
    *,
};
use herdr_client::{Method, protocol::ClientShellSnapshot};
use serde_json::{Value, json};

#[derive(Clone)]
struct Target {
    boot: String,
    workspace: String,
    tab: String,
    pane: String,
    label: String,
}

impl Target {
    fn capture(snapshot: &ClientShellSnapshot, id: &str) -> Option<Self> {
        let pane = snapshot.panes.iter().find(|pane| pane.pane_id == id)?;
        let target = Self {
            boot: snapshot.boot_id.clone(),
            workspace: pane.workspace_id.clone(),
            tab: pane.tab_id.clone(),
            pane: pane.pane_id.clone(),
            label: pane.label.clone().unwrap_or_default(),
        };
        target.validate(snapshot).ok()?;
        Some(target)
    }

    fn validate(&self, snapshot: &ClientShellSnapshot) -> crate::Result<()> {
        if snapshot.boot_id != self.boot
            || !snapshot
                .workspaces
                .iter()
                .any(|w| w.workspace_id == self.workspace)
            || !snapshot
                .tabs
                .iter()
                .any(|t| t.tab_id == self.tab && t.workspace_id == self.workspace)
            || !snapshot.panes.iter().any(|p| {
                p.pane_id == self.pane && p.tab_id == self.tab && p.workspace_id == self.workspace
            })
        {
            return Err(crate::Error::StalePane);
        }
        Ok(())
    }

    fn rename_params(&self, label: &str) -> Value {
        json!({"pane_id": self.pane, "label": label.trim()})
    }
}

#[derive(Clone, Copy)]
enum Action {
    Rename,
    SplitRight,
    SplitDown,
    Zoom,
    Close,
}

impl Action {
    fn request(self, target: &Target) -> Option<(Method, Value)> {
        Some(match self {
            Self::SplitRight | Self::SplitDown => (
                Method::PaneSplit,
                json!({
                    "target_pane_id": target.pane,
                    "direction": if matches!(self, Self::SplitRight) { "right" } else { "down" },
                    "focus": true,
                }),
            ),
            Self::Zoom => (
                Method::PaneZoom,
                json!({"pane_id": target.pane, "mode": "toggle"}),
            ),
            Self::Rename | Self::Close => return None,
        })
    }
}

const ACTIONS: [(Action, &str); 5] = [
    (Action::Rename, "Rename"),
    (Action::SplitRight, "Split Right"),
    (Action::SplitDown, "Split Down"),
    (Action::Zoom, "Toggle Zoom"),
    (Action::Close, "Close"),
];

pub(crate) struct PaneMenu {
    target: Target,
    pending: Option<String>,
    error: Option<String>,
}

impl Action {
    fn icon(self) -> Icon {
        match self {
            Self::Rename => Icon::default().path("icons/pencil.svg"),
            Self::SplitRight => Icon::new(IconName::PanelRight),
            Self::SplitDown => Icon::new(IconName::PanelBottom),
            Self::Zoom => Icon::new(IconName::Maximize),
            Self::Close => Icon::new(IconName::Close),
        }
    }
}

impl HerdrWindow {
    pub(crate) fn open_pane_menu_at(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pressed_terminal_link = None;
        if self.menu_is_open() || !self.input_ready() {
            return;
        }
        let Some(surface) = &self.live.surface else {
            return;
        };
        let Some(id) = crate::terminal::pane_at(
            surface,
            self.bounds,
            position,
            self.cell_width,
            self.config.terminal.line_height(),
        ) else {
            return;
        };
        let id = id.to_owned();
        self.open_pane_menu(&id, position, window, cx);
    }

    fn open_pane_menu(
        &mut self,
        id: &str,
        anchor: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self
            .live
            .snapshot
            .as_ref()
            .and_then(|s| Target::capture(s, id))
        else {
            return;
        };
        if !self.begin_menu(window, cx) {
            return;
        }
        self.menu.pane = Some(PaneMenu {
            target,
            pending: None,
            error: None,
        });
        let weak = cx.weak_entity();
        self.show_popup(Page::Pane, anchor, window, cx, move |menu, _, _| {
            ACTIONS.into_iter().fold(menu, |menu, (action, label)| {
                let menu = if matches!(action, Action::Close) {
                    menu.separator()
                } else {
                    menu
                };
                menu.item(
                    PopupMenuItem::new(label)
                        .icon(action.icon())
                        .on_click(listener(&weak, move |this, window, cx| {
                            this.activate_pane_menu(action, window, cx)
                        })),
                )
            })
        });
    }

    fn validate_pane_target(&self) -> crate::Result<&Target> {
        if !self.menu_target_current() {
            return Err(crate::Error::StaleConnection);
        }
        let target = &self
            .menu
            .pane
            .as_ref()
            .ok_or(crate::Error::StalePane)?
            .target;
        target.validate(
            self.live
                .snapshot
                .as_ref()
                .ok_or(crate::Error::NotConnected)?,
        )?;
        Ok(target)
    }

    fn pane_error(&mut self, error: impl std::fmt::Display, cx: &mut Context<Self>) {
        if let Some(pane) = &mut self.menu.pane {
            pane.error = Some(error.to_string());
        }
        cx.notify();
    }

    /// A menu row. The popup closes with the click, so a refusal to act is
    /// reported as the window's error rather than inside a closed menu.
    fn activate_pane_menu(&mut self, action: Action, window: &mut Window, cx: &mut Context<Self>) {
        let target = match self.validate_pane_target() {
            Ok(target) => target.clone(),
            Err(error) => {
                self.local_error = Some(error.to_string());
                self.dismiss_menu(window, cx);
                return;
            }
        };
        match action {
            Action::Rename => self.rename_pane(target.label, window, cx),
            Action::Close => {
                // Keep the original endpoint fence, rather than reopening the menu.
                self.menu.close = self
                    .live
                    .snapshot
                    .as_ref()
                    .and_then(|s| CloseConfirmation::capture_pane(s, &target.pane));
                if self.menu.close.is_some() {
                    self.show_close_confirmation(window, cx);
                } else {
                    self.dismiss_menu(window, cx);
                }
            }
            action => {
                let result = (|| {
                    let (method, params) = action
                        .request(&target)
                        .ok_or(crate::Error::UnsupportedCommand)?;
                    if !self.input_ready()
                        || self
                            .live
                            .surface
                            .as_ref()
                            .is_some_and(|s| s.popup.is_some())
                    {
                        return Err(crate::Error::ConnectionNotReady);
                    }
                    let handle = self.endpoints[self.selected_endpoint]
                        .connection
                        .handle
                        .as_ref()
                        .ok_or(crate::Error::NotConnected)?;
                    handle.request(&target.boot, method, params)?;
                    Ok::<_, crate::Error>(())
                })();
                match result {
                    Ok(()) => self.fence_focus_change(None),
                    Err(error) => self.local_error = Some(error.to_string()),
                }
                self.dismiss_menu(window, cx);
            }
        }
    }

    fn rename_pane(&mut self, label: String, window: &mut Window, cx: &mut Context<Self>) {
        self.set_menu_input(label, "Pane name (blank clears)", window, cx);
        self.show_dialog(Page::RenamePane, window, cx, |this, dialog, weak, _, _| {
            let Some(pane) = &this.menu.pane else {
                return dialog;
            };
            let pending = pane.pending.is_some();
            dialog
                .title("Rename pane")
                .child(
                    v_flex()
                        .gap_3()
                        .child("Leave blank to clear the custom label.")
                        .children(
                            this.menu
                                .input
                                .as_ref()
                                .map(|input| Input::new(input).disabled(pending)),
                        )
                        .children(error_alert("pane-rename-error", pane.error.as_ref())),
                )
                .footer(dialog_buttons(
                    weak,
                    Some(
                        Button::new("pane-rename-submit")
                            .primary()
                            .label(if pending { "Renaming..." } else { "Rename" })
                            .loading(pending)
                            .on_click(listener(weak, |this, window, cx| {
                                this.submit_pane_rename(window, cx)
                            })),
                    ),
                ))
                .on_ok(submit(weak, |this, window, cx| {
                    this.submit_pane_rename(window, cx)
                }))
        });
        self.focus_menu_input(window, cx);
    }

    fn submit_pane_rename(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(pane) = &self.menu.pane else { return };
        if pane.pending.is_some() {
            return;
        }
        let text = self.menu.input_text(cx);
        let result = (|| {
            let target = self.validate_pane_target()?;
            if !self.input_ready()
                || self
                    .live
                    .surface
                    .as_ref()
                    .is_some_and(|s| s.popup.is_some())
            {
                return Err(crate::Error::ConnectionNotReady);
            }
            let endpoint = &self.endpoints[self.selected_endpoint];
            let handle = endpoint
                .connection
                .handle
                .as_ref()
                .ok_or(crate::Error::NotConnected)?;
            let mut inbox = endpoint
                .connection
                .inbox
                .try_lock()
                .map_err(|_| crate::Error::ConnectionBusy)?;
            // Register under the reducer's lock so even an immediate reply is retained.
            let request = handle.request(
                &target.boot,
                Method::PaneRename,
                target.rename_params(&text),
            )?;
            inbox.pane_rename = Some(crate::state::RenameResult {
                request: request.clone(),
                result: None,
            });
            Ok::<_, crate::Error>(request)
        })();
        match result {
            Ok(request) => {
                if let Some(pane) = &mut self.menu.pane {
                    pane.pending = Some(request);
                    pane.error = None;
                }
                cx.notify();
            }
            Err(error) => self.pane_error(error, cx),
        }
    }

    pub(crate) fn poll_pane_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(request) = self
            .menu
            .pane
            .as_ref()
            .and_then(|pane| pane.pending.as_ref())
        else {
            return;
        };
        let result = if let Err(error) = self.validate_pane_target() {
            Some(Err(std::sync::Arc::new(error)))
        } else {
            self.live
                .pane_rename
                .as_ref()
                .filter(|rename| &rename.request == request)
                .and_then(|rename| rename.result.clone())
        };
        match result {
            Some(Ok(())) => self.dismiss_menu(window, cx),
            Some(Err(error)) => {
                if let Some(pane) = &mut self.menu.pane {
                    pane.pending = None;
                }
                self.focus_menu_input(window, cx);
                self.pane_error(error, cx);
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use core::prelude::v1::test;

    fn snapshot() -> ClientShellSnapshot {
        let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
            "../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
        ))
        .unwrap();
        let mut pane = snapshot.panes[0].clone();
        pane.pane_id = "inactive".into();
        pane.label = Some("Original label".into());
        snapshot.panes.push(pane);
        snapshot
    }

    #[test]
    fn captured_pane_payloads_ignore_focus_and_reject_stale_membership() {
        let original = snapshot();
        let target = Target::capture(&original, "inactive").unwrap();
        let mut snapshot = original.clone();
        snapshot.focused_pane_id = None;
        snapshot.focused_tab_id = None;
        snapshot.focused_workspace_id = None;
        assert!(target.validate(&snapshot).is_ok());
        assert_eq!(
            target.rename_params("  \u{4e2d}  "),
            json!({"pane_id":"inactive", "label":"\u{4e2d}"})
        );
        assert_eq!(
            target.rename_params(" \u{2003}\t"),
            json!({"pane_id":"inactive", "label":""})
        );
        for (action, direction) in [(Action::SplitRight, "right"), (Action::SplitDown, "down")] {
            assert_eq!(
                action.request(&target).unwrap(),
                (
                    Method::PaneSplit,
                    json!({"target_pane_id":"inactive", "direction":direction, "focus":true})
                )
            );
        }
        assert_eq!(
            Action::Zoom.request(&target).unwrap(),
            (
                Method::PaneZoom,
                json!({"pane_id":"inactive", "mode":"toggle"})
            )
        );
        for case in 0..7 {
            let mut snapshot = original.clone();
            match case {
                0 => snapshot.boot_id.push('x'),
                1 => snapshot.workspaces.clear(),
                2 => snapshot.tabs.clear(),
                3 => snapshot.tabs[0].workspace_id.push('x'),
                4 => snapshot.panes[1].tab_id.push('x'),
                5 => snapshot.panes[1].workspace_id.push('x'),
                _ => snapshot.panes.truncate(1),
            }
            assert!(matches!(
                target.validate(&snapshot),
                Err(crate::Error::StalePane)
            ));
        }
    }
}
