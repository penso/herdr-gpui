//! A tab's context menu and its rename dialog. The target is captured when
//! the menu opens, so a rename lands on the tab that was clicked even after
//! focus moves elsewhere.

use crate::{
    HerdrWindow,
    menu::{Page, dialog_buttons, error_alert, listener, submit},
};
use gpui_kit::{
    component::{
        Icon,
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
    label: String,
}

impl Target {
    fn capture(snapshot: &ClientShellSnapshot, id: &str) -> Option<Self> {
        let tab = snapshot.tabs.iter().find(|tab| tab.tab_id == id)?;
        Some(Self {
            boot: snapshot.boot_id.clone(),
            workspace: tab.workspace_id.clone(),
            tab: tab.tab_id.clone(),
            label: tab.label.clone(),
        })
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
        {
            return Err(crate::Error::StaleTab);
        }
        Ok(())
    }

    fn rename_params(&self, label: &str) -> crate::Result<Value> {
        let label = label.trim();
        if label.is_empty() {
            return Err(crate::Error::EmptyTabName);
        }
        Ok(json!({"tab_id": self.tab, "label": label}))
    }
}

pub(crate) struct TabMenu {
    target: Target,
    pending: Option<String>,
    error: Option<String>,
}

impl HerdrWindow {
    pub(crate) fn open_tab_menu(
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
        self.menu.tab = Some(TabMenu {
            target,
            pending: None,
            error: None,
        });
        let weak = cx.weak_entity();
        self.show_popup(Page::Tab, anchor, window, cx, move |menu, _, _| {
            menu.item(
                PopupMenuItem::new("Rename")
                    .icon(Icon::default().path("icons/pencil.svg"))
                    .on_click(listener(&weak, |this, window, cx| {
                        this.rename_tab(window, cx)
                    })),
            )
        });
    }

    fn validate_tab_target(&self) -> crate::Result<&Target> {
        if !self.menu_target_current() {
            return Err(crate::Error::StaleConnection);
        }
        let target = &self.menu.tab.as_ref().ok_or(crate::Error::NoTab)?.target;
        target.validate(
            self.live
                .snapshot
                .as_ref()
                .ok_or(crate::Error::NotConnected)?,
        )?;
        Ok(target)
    }

    fn tab_error(&mut self, error: impl std::fmt::Display, cx: &mut Context<Self>) {
        if let Some(tab) = &mut self.menu.tab {
            tab.error = Some(error.to_string());
        }
        cx.notify();
    }

    fn rename_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let label = match self.validate_tab_target() {
            Ok(target) => target.label.clone(),
            Err(error) => {
                // The menu closes with the click, so the refusal is the window's.
                self.local_error = Some(error.to_string());
                self.dismiss_menu(window, cx);
                return;
            }
        };
        self.set_menu_input(label, "Tab name", window, cx);
        self.show_dialog(Page::RenameTab, window, cx, |this, dialog, weak, _, _| {
            let Some(tab) = &this.menu.tab else {
                return dialog;
            };
            let pending = tab.pending.is_some();
            dialog
                .title("Rename tab")
                .child(
                    v_flex()
                        .gap_3()
                        .children(
                            this.menu
                                .input
                                .as_ref()
                                .map(|input| Input::new(input).disabled(pending)),
                        )
                        .children(error_alert("tab-rename-error", tab.error.as_ref())),
                )
                .footer(dialog_buttons(
                    weak,
                    Some(
                        Button::new("tab-rename-submit")
                            .primary()
                            .label(if pending { "Renaming..." } else { "Rename" })
                            .loading(pending)
                            .on_click(listener(weak, |this, window, cx| {
                                this.submit_tab_rename(window, cx)
                            })),
                    ),
                ))
                .on_ok(submit(weak, |this, window, cx| {
                    this.submit_tab_rename(window, cx)
                }))
        });
        self.focus_menu_input(window, cx);
    }

    fn submit_tab_rename(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = &self.menu.tab else { return };
        if tab.pending.is_some() {
            return;
        }
        let text = self.menu.input_text(cx);
        let result = (|| {
            let target = self.validate_tab_target()?;
            let params = target.rename_params(&text)?;
            if !self.input_ready() {
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
            // Install correlation under the same lock used by the event reducer.
            let request = handle.request(&target.boot, Method::TabRename, params)?;
            inbox.tab_rename = Some(crate::state::RenameResult {
                request: request.clone(),
                result: None,
            });
            Ok::<_, crate::Error>(request)
        })();
        match result {
            Ok(request) => {
                if let Some(tab) = &mut self.menu.tab {
                    tab.pending = Some(request);
                    tab.error = None;
                }
                cx.notify();
            }
            Err(error) => self.tab_error(error, cx),
        }
    }

    pub(crate) fn poll_tab_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(request) = self.menu.tab.as_ref().and_then(|tab| tab.pending.as_ref()) else {
            return;
        };
        let result = if let Err(error) = self.validate_tab_target() {
            Some(Err(std::sync::Arc::new(error)))
        } else {
            self.live
                .tab_rename
                .as_ref()
                .filter(|r| &r.request == request)
                .and_then(|r| r.result.clone())
        };
        match result {
            Some(Ok(())) => self.dismiss_menu(window, cx),
            Some(Err(error)) => {
                if let Some(tab) = &mut self.menu.tab {
                    tab.pending = None;
                }
                self.focus_menu_input(window, cx);
                self.tab_error(error, cx);
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

    #[test]
    fn tab_target_retains_membership_and_rejects_stale_snapshots() {
        let original: ClientShellSnapshot = serde_json::from_str(include_str!(
            "../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
        ))
        .unwrap();
        let target = Target::capture(&original, &original.tabs[0].tab_id).unwrap();
        let mut snapshot = original.clone();
        snapshot.focused_tab_id = None;
        snapshot.focused_workspace_id = None;
        assert!(target.validate(&snapshot).is_ok());
        assert_eq!(
            target.rename_params("  \u{4e2d}  ").unwrap(),
            json!({"tab_id": target.tab, "label": "\u{4e2d}"})
        );
        assert!(target.rename_params(" \u{2003}\t").is_err());
        snapshot.boot_id.push_str("-replaced");
        assert!(target.validate(&snapshot).is_err());
        snapshot = original.clone();
        snapshot.tabs[0].workspace_id.push_str("-moved");
        assert!(target.validate(&snapshot).is_err());
        snapshot = original.clone();
        snapshot.workspaces.clear();
        assert!(target.validate(&snapshot).is_err());
        snapshot = original;
        snapshot.tabs.clear();
        assert!(target.validate(&snapshot).is_err());
    }
}
