use crate::{
    HerdrWindow,
    controls::{self, Command},
    menu::Page,
};
use gpui::{prelude::*, *};
use herdr_client::protocol::ClientShellSnapshot;
use serde_json::{Value, json};

pub(super) struct CloseConfirmation {
    boot: String,
    workspace: String,
    tab: String,
    pane: Option<String>,
    label: String,
    confirm_selected: bool,
    error: Option<String>,
}

impl CloseConfirmation {
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
            confirm_selected: false,
            error: None,
        })
    }

    fn request(&self, snapshot: &ClientShellSnapshot) -> Result<(&'static str, Value), String> {
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
            return Err(
                "The original target changed or no longer exists. Cancel and try again.".into(),
            );
        }
        Ok(if let Some(id) = &self.pane {
            ("pane.close", json!({"pane_id": id}))
        } else {
            ("tab.close", json!({"tab_id": self.tab}))
        })
    }
}

impl HerdrWindow {
    pub(super) fn open_close_confirmation(
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
        self.open_menu(window, cx);
        self.menu.close = Some(close);
        self.menu.page = Some(Page::ConfirmClose);
    }

    fn confirm_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(close) = &self.menu.close else {
            return;
        };
        let result = (|| {
            if !self.menu_target_current() || !self.input_ready() {
                return Err(
                    "The selected connection changed or is not ready. Cancel and try again.".into(),
                );
            }
            let snapshot = self
                .live
                .snapshot
                .as_ref()
                .ok_or("Not connected to a daemon.")?;
            close.request(snapshot)
        })();
        match result {
            Ok((method, params)) => {
                self.request_focus_change(
                    method,
                    None,
                    |handle, boot| handle.request(boot, method, params),
                    cx,
                );
                self.dismiss_menu(window, cx);
            }
            Err(error) => {
                if let Some(close) = &mut self.menu.close {
                    close.error = Some(error);
                }
                cx.notify();
            }
        }
    }

    pub(super) fn close_confirmation_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        window.prevent_default();
        match event.keystroke.key.as_str() {
            "escape" => self.dismiss_menu(window, cx),
            "tab" | "left" | "right" => {
                if let Some(close) = &mut self.menu.close {
                    close.confirm_selected = !close.confirm_selected;
                }
                cx.notify();
            }
            "enter" => {
                if self
                    .menu
                    .close
                    .as_ref()
                    .is_some_and(|close| close.confirm_selected)
                {
                    self.confirm_close(window, cx);
                } else {
                    self.dismiss_menu(window, cx);
                }
            }
            _ => {}
        }
    }

    pub(super) fn render_close_confirmation(&self, cx: &mut Context<Self>) -> Div {
        let Some(close) = &self.menu.close else {
            return div();
        };
        let theme = &self.theme;
        let kind = if close.pane.is_some() { "pane" } else { "tab" };
        div().p(px(12.)).flex().flex_col().gap(px(12.))
            .child(div().text_size(px(self.config.ui.size * 1.35)).font_weight(FontWeight::SEMIBOLD).child(format!("Close {kind}?")))
            .child(div().child(close.label.clone()))
            .child(div().text_color(rgb(theme.muted)).child(if close.pane.is_some() {
                "This terminates the pane and its running processes. This cannot be undone."
            } else { "This terminates every pane and running process in this tab. This cannot be undone." }))
            .when_some(close.error.clone(), |panel, error| panel.child(div().bg(rgb(theme.active)).p(px(8.)).child(error)))
            .child(div().flex().justify_end().gap(px(8.))
                .child(div().id("close-cancel").debug_selector(|| "close-cancel".into()).px(px(12.)).py(px(6.)).rounded(px(4.)).border_1()
                    .border_color(rgb(if close.confirm_selected { theme.active } else { theme.foreground }))
                    .cursor_pointer().hover(|s| s.bg(rgb(theme.active))).child("Cancel")
                    .on_click(cx.listener(|this, _, window, cx| this.dismiss_menu(window, cx))))
                .child(div().id("close-confirm").debug_selector(|| "close-confirm".into()).px(px(12.)).py(px(6.)).rounded(px(4.)).border_1()
                    .border_color(rgb(if close.confirm_selected { theme.foreground } else { theme.active }))
                    .bg(rgb(theme.active)).cursor_pointer().child(format!("Close {kind}"))
                    .on_click(cx.listener(|this, _, window, cx| this.confirm_close(window, cx)))))
            .child(div().text_color(rgb(theme.muted)).child("Tab to choose, Enter to activate. Escape to cancel."))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    #[test]
    fn close_retains_original_target_and_rejects_replaced_sessions() -> Result<(), String> {
        let mut snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
            "../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
        ))
        .map_err(|error| error.to_string())?;
        for command in [Command::ClosePane, Command::CloseTab] {
            let close = CloseConfirmation::capture(command, &snapshot).ok_or("missing target")?;
            assert!(!close.confirm_selected, "Cancel is the safe default");
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
