//! Confirmation before closing a pane or tab, which terminates its running
//! processes. The target is captured when the prompt opens and revalidated
//! when it is confirmed.

use crate::{
    Error, HerdrWindow, Result,
    controls::{self, Command},
    menu::{Page, dialog_buttons, error_alert, listener, submit},
};
use gpui_kit::{
    component::{
        ActiveTheme as _,
        button::{Button, ButtonVariants as _},
        v_flex,
    },
    prelude::*,
    *,
};
use herdr_client::{Method, protocol::ClientShellSnapshot};
use serde_json::{Value, json};

pub(crate) struct CloseConfirmation {
    boot: String,
    workspace: String,
    tab: String,
    pane: Option<String>,
    label: String,
    error: Option<String>,
}

impl CloseConfirmation {
    pub(super) fn capture_pane(snapshot: &ClientShellSnapshot, id: &str) -> Option<Self> {
        let pane = snapshot.panes.iter().find(|pane| pane.pane_id == id)?;
        let mut close = Self::capture_tab(snapshot, &pane.tab_id)?;
        if close.workspace != pane.workspace_id {
            return None;
        }
        close.pane = Some(pane.pane_id.clone());
        close.label = pane.label.clone().unwrap_or_else(|| pane.pane_id.clone());
        close.request(snapshot).ok()?;
        Some(close)
    }

    pub(super) fn capture_tab(snapshot: &ClientShellSnapshot, id: &str) -> Option<Self> {
        let tab = snapshot.tabs.iter().find(|tab| tab.tab_id == id)?;
        Some(Self {
            boot: snapshot.boot_id.clone(),
            workspace: tab.workspace_id.clone(),
            tab: tab.tab_id.clone(),
            pane: None,
            label: tab.label.clone(),
            error: None,
        })
    }

    fn capture(command: Command, snapshot: &ClientShellSnapshot) -> Option<Self> {
        if !matches!(command, Command::ClosePane | Command::CloseTab) {
            return None;
        }
        controls::request(command, snapshot)?;
        let tab = snapshot
            .tabs
            .iter()
            .find(|tab| Some(&tab.tab_id) == snapshot.focused_tab_id.as_ref())?;
        let pane = if command == Command::ClosePane {
            Some(
                snapshot
                    .panes
                    .iter()
                    .find(|pane| Some(&pane.pane_id) == snapshot.focused_pane_id.as_ref())?,
            )
        } else {
            None
        };
        Some(Self {
            boot: snapshot.boot_id.clone(),
            workspace: tab.workspace_id.clone(),
            tab: tab.tab_id.clone(),
            pane: pane.map(|pane| pane.pane_id.clone()),
            label: pane
                .map(|pane| pane.label.clone().unwrap_or_else(|| pane.pane_id.clone()))
                .unwrap_or_else(|| tab.label.clone()),
            error: None,
        })
    }

    fn request(&self, snapshot: &ClientShellSnapshot) -> Result<(Method, Value)> {
        if snapshot.boot_id != self.boot
            || !snapshot
                .workspaces
                .iter()
                .any(|workspace| workspace.workspace_id == self.workspace)
            || !snapshot
                .tabs
                .iter()
                .any(|tab| tab.tab_id == self.tab && tab.workspace_id == self.workspace)
            || self.pane.as_ref().is_some_and(|id| {
                !snapshot.panes.iter().any(|pane| {
                    &pane.pane_id == id
                        && pane.tab_id == self.tab
                        && pane.workspace_id == self.workspace
                })
            })
        {
            return Err(Error::StaleCloseTarget);
        }
        Ok(if let Some(id) = &self.pane {
            (Method::PaneClose, json!({"pane_id": id}))
        } else {
            (Method::TabClose, json!({"tab_id": self.tab}))
        })
    }
}

impl HerdrWindow {
    pub(crate) fn open_tab_close(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(close) = self
            .live
            .snapshot
            .as_ref()
            .and_then(|snapshot| CloseConfirmation::capture_tab(snapshot, id))
        else {
            return;
        };
        if !self.begin_menu(window, cx) {
            return;
        }
        self.menu.close = Some(close);
        self.show_close_confirmation(window, cx);
        if !self.config.confirm_close_tab {
            self.confirm_close(window, cx);
        }
    }

    pub(crate) fn open_close_confirmation(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(close) = self
            .live
            .snapshot
            .as_ref()
            .and_then(|snapshot| CloseConfirmation::capture(command, snapshot))
        else {
            return;
        };
        if !self.begin_menu(window, cx) {
            return;
        }
        self.menu.close = Some(close);
        self.show_close_confirmation(window, cx);
        if command == Command::CloseTab && !self.config.confirm_close_tab {
            self.confirm_close(window, cx);
        }
    }

    /// Shows the prompt for the captured `menu.close`. The safe choice is the
    /// default: Enter confirms only through the destructive button itself.
    pub(crate) fn show_close_confirmation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_dialog(Page::ConfirmClose, window, cx, |this, dialog, weak, _, cx| {
            let Some(close) = &this.menu.close else {
                return dialog;
            };
            let kind = if close.pane.is_some() { "pane" } else { "tab" };
            dialog
                .title(format!("Close {kind}?"))
                .child(
                    v_flex()
                        .gap_2()
                        .child(close.label.clone())
                        .child(div().text_color(cx.theme().muted_foreground).child(
                            if close.pane.is_some() {
                                "This terminates the pane and its running processes. This cannot be undone."
                            } else {
                                "This terminates every pane and running process in this tab. This cannot be undone."
                            },
                        ))
                        .children(error_alert("close-error", close.error.as_ref())),
                )
                .footer(dialog_buttons(
                    weak,
                    Some(
                        Button::new("close-confirm")
                            .danger()
                            .label(format!("Close {kind}"))
                            .on_click(listener(weak, |this, window, cx| {
                                this.confirm_close(window, cx)
                            })),
                    ),
                ))
                .on_ok(submit(weak, |this, window, cx| this.dismiss_menu(window, cx)))
        });
    }

    pub(crate) fn confirm_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(close) = &self.menu.close else {
            return;
        };
        let result = (|| {
            if !self.menu_target_current() || !self.input_ready() {
                return Err(Error::StaleConnection);
            }
            if close.pane.is_some()
                && self
                    .live
                    .surface
                    .as_ref()
                    .is_some_and(|s| s.popup.is_some())
            {
                return Err(Error::ConnectionNotReady);
            }
            let snapshot = self.live.snapshot.as_ref().ok_or(Error::NotConnected)?;
            close.request(snapshot)
        })();
        match result {
            Ok((method, params)) => {
                self.request_focus_change(method.as_str(), None, |handle, boot| {
                    handle.request(boot, method, params)
                });
                self.dismiss_menu(window, cx);
            }
            Err(error) => {
                if let Some(close) = &mut self.menu.close {
                    close.error = Some(error.to_string());
                }
                cx.notify();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use core::prelude::v1::test;
    use std::sync::Arc;

    #[gpui_kit::test]
    fn skipping_tab_confirmation_keeps_connection_checks_and_pane_prompt(cx: &mut TestAppContext) {
        let (view, cx) = crate::test_support::add_window_view(cx, |window, cx| {
            let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
            view.config.confirm_close_tab = false;
            view.live.snapshot = Some(Arc::new(
                serde_json::from_str(include_str!(
                    "../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
                ))
                .unwrap(),
            ));
            view
        });
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let id = view.live.snapshot.as_ref().unwrap().tabs[0].tab_id.clone();
                view.open_tab_close(&id, window, cx);
                // The disconnected fixture must attempt the close immediately but refuse to send it.
                assert!(view.menu.close.as_ref().unwrap().error.is_some());
                assert!(view.pending_navigation.is_none());
                view.dismiss_menu(window, cx);
                view.open_close_confirmation(Command::CloseTab, window, cx);
                assert!(view.menu.close.as_ref().unwrap().error.is_some());
                view.dismiss_menu(window, cx);
                view.open_close_confirmation(Command::ClosePane, window, cx);
                assert!(view.menu.close.as_ref().unwrap().error.is_none());
                view.dismiss_menu(window, cx);
                view.config.confirm_close_tab = true;
                view.open_tab_close(&id, window, cx);
                assert!(view.menu.close.as_ref().unwrap().error.is_none());
            })
        });
    }

    #[test]
    fn explicit_pane_close_retains_inactive_target_and_membership() {
        let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
            "../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
        ))
        .unwrap();
        let mut pane = snapshot.panes[0].clone();
        pane.pane_id = "inactive".into();
        snapshot.panes.push(pane);
        let close = CloseConfirmation::capture_pane(&snapshot, "inactive").unwrap();
        snapshot.focused_pane_id = None;
        assert_eq!(
            close.request(&snapshot).unwrap(),
            (Method::PaneClose, json!({"pane_id":"inactive"}))
        );
        let original = snapshot.clone();
        for case in 0..6 {
            let mut snapshot = original.clone();
            match case {
                0 => snapshot.boot_id.push('x'),
                1 => snapshot.workspaces.clear(),
                2 => snapshot.tabs.clear(),
                3 => snapshot.panes[1].tab_id.push('x'),
                4 => snapshot.panes[1].workspace_id.push('x'),
                _ => snapshot.panes.truncate(1),
            }
            assert!(matches!(
                close.request(&snapshot),
                Err(Error::StaleCloseTarget)
            ));
        }
    }

    #[test]
    fn explicit_tab_close_does_not_follow_focus() -> anyhow::Result<()> {
        let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
            "../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
        ))?;
        let mut inactive = snapshot.tabs[0].clone();
        inactive.tab_id = "inactive".into();
        inactive.focused = false;
        snapshot.tabs.push(inactive);
        let close = CloseConfirmation::capture_tab(&snapshot, "inactive")
            .ok_or_else(|| anyhow::anyhow!("missing tab"))?;
        assert_eq!(
            close.request(&snapshot)?,
            (Method::TabClose, json!({"tab_id":"inactive"}))
        );
        snapshot.tabs.retain(|tab| tab.tab_id != "inactive");
        assert!(close.request(&snapshot).is_err());
        Ok(())
    }
    #[test]
    fn close_retains_original_target_and_rejects_replaced_sessions() -> anyhow::Result<()> {
        let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
            "../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
        ))?;
        for command in [Command::ClosePane, Command::CloseTab] {
            let close = CloseConfirmation::capture(command, &snapshot)
                .ok_or_else(|| anyhow::anyhow!("missing target"))?;
            let expected = close.request(&snapshot)?;
            let original = snapshot.clone();
            snapshot.focused_pane_id = None;
            snapshot.focused_tab_id = None;
            assert_eq!(close.request(&snapshot)?, expected);
            snapshot.boot_id = "new-boot".into();
            assert!(close.request(&snapshot).is_err());
            snapshot = original.clone();
            snapshot.tabs.clear();
            assert!(close.request(&snapshot).is_err());
            snapshot = original;
        }
        Ok(())
    }
}
