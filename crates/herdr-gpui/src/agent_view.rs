//! GUI-only agent presentation. Herdr remains the owner of every terminal.
use super::{
    HerdrWindow, NavigationTarget, OwnedNavigationTarget,
    agent_mode::{self, TabTarget, ViewMode},
    composer,
};
use gpui::{prelude::*, *};
use herdr_client::protocol::{
    ClientKeyCode, ClientKeyKind, ClientPaneInputEvent, ClientShellSnapshot,
};

pub(super) struct NavigationFence {
    request_id: String,
    goal: Option<OwnedNavigationTarget>,
    boot_id: String,
    workspace_id: Option<String>,
    tab_id: Option<String>,
    pane_id: Option<String>,
}

impl NavigationFence {
    pub fn new(
        snapshot: &ClientShellSnapshot,
        request_id: String,
        goal: Option<OwnedNavigationTarget>,
    ) -> Self {
        Self {
            request_id,
            goal,
            boot_id: snapshot.boot_id.clone(),
            workspace_id: snapshot.focused_workspace_id.clone(),
            tab_id: snapshot.focused_tab_id.clone(),
            pane_id: snapshot.focused_pane_id.clone(),
        }
    }
}

fn prompt_events(text: &str) -> [ClientPaneInputEvent; 2] {
    [
        ClientPaneInputEvent::Paste(text.to_owned()),
        ClientPaneInputEvent::Key {
            code: ClientKeyCode::Enter,
            modifiers: 0,
            kind: ClientKeyKind::Press,
            repeat_count: 1,
            shifted_codepoint: None,
            generated_text: None,
            tracks_release: false,
            physical_key_id: None,
            windows_record: None,
        },
    ]
}

impl HerdrWindow {
    pub(super) fn agent_tab_active(&self) -> bool {
        self.live.snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.focused_tab_id.as_ref().is_some_and(|tab| {
                self.agent_modes.mode(
                    &self.endpoints[self.selected_endpoint].id,
                    &snapshot.boot_id,
                    tab,
                ) == ViewMode::Agent
            })
        })
    }

    pub(super) fn sync_composer(&mut self, cx: &mut Context<Self>) {
        let endpoint_id = &self.endpoints[self.selected_endpoint].id;
        self.agent_modes
            .retain_endpoints(|id| self.endpoints.iter().any(|endpoint| endpoint.id == id));
        if let Some(snapshot) = &self.live.snapshot {
            self.agent_modes.reconcile(endpoint_id, snapshot);
            if self.navigation_fence.as_ref().is_some_and(|fence| {
                fence.boot_id != snapshot.boot_id
                    || self.live.request_status(&fence.request_id)
                        == Some(crate::state::RequestStatus::Failed)
                    || match &fence.goal {
                        Some(NavigationTarget::Workspace(id)) => {
                            snapshot.focused_workspace_id.as_ref() == Some(id)
                        }
                        Some(NavigationTarget::Tab(id)) => {
                            snapshot.focused_tab_id.as_ref() == Some(id)
                        }
                        Some(NavigationTarget::Pane(id)) => {
                            snapshot.focused_pane_id.as_ref() == Some(id)
                        }
                        None => {
                            fence.workspace_id != snapshot.focused_workspace_id
                                || fence.tab_id != snapshot.focused_tab_id
                                || fence.pane_id != snapshot.focused_pane_id
                                || (self.live.request_status(&fence.request_id)
                                    == Some(crate::state::RequestStatus::Succeeded)
                                    && self.input_ready())
                        }
                    }
            }) {
                self.navigation_fence = None;
            }
        }
        let target = self
            .live
            .snapshot
            .as_ref()
            .and_then(|snapshot| self.agent_modes.focused_target(endpoint_id, snapshot));
        if target != self.composer_target {
            self.composer
                .update(cx, |editor, cx| editor.cancel_composition(cx));
            if let Some(previous) = self.composer_target.take() {
                let draft = self.composer.read(cx).draft();
                if draft.text().is_empty() {
                    self.drafts.remove(&previous);
                } else {
                    self.drafts.insert(previous, draft);
                }
            }
            let draft = target
                .as_ref()
                .and_then(|target| self.drafts.remove(target))
                .unwrap_or_default();
            self.composer
                .update(cx, |editor, cx| editor.set_draft(draft, cx));
            self.composer_target = target;
            self.composer_notice = None;
            self.marked.clear();
        }
        self.drafts.retain(|target, _| {
            self.endpoints
                .iter()
                .any(|endpoint| endpoint.id == target.endpoint_id)
        });
        // A disconnected inbox is not evidence of deletion. Other hosts' drafts
        // must not be compared with the selected host's boot or pane membership.
        if let Some(snapshot) = &self.live.snapshot {
            self.drafts.retain(|target, _| {
                target.endpoint_id != *endpoint_id
                    || (target.boot_id == snapshot.boot_id
                        && snapshot.tabs.iter().any(|tab| tab.tab_id == target.tab_id)
                        && snapshot.panes.iter().any(|pane| {
                            pane.pane_id == target.pane_id && pane.tab_id == target.tab_id
                        }))
            });
        }
        let enabled = self.composer_editable();
        self.composer
            .update(cx, |editor, cx| editor.set_enabled(enabled, cx));
    }

    // Editing a local draft does not require a freshly rendered surface. In
    // particular, a snapshot/surface gap must not hand typing to the terminal.
    pub(super) fn composer_editable(&self) -> bool {
        self.composer_target.is_some()
            && !self.menu.is_open()
            && self.navigation_fence.is_none()
            && self
                .live
                .surface
                .as_ref()
                .is_none_or(|surface| surface.popup.is_none())
    }

    pub(super) fn composer_block_reason(&self) -> Option<&'static str> {
        if self.menu.is_open() {
            return Some("Close the menu to edit your draft.");
        }
        if self.endpoints[self.selected_endpoint]
            .connection
            .handle
            .is_none()
            || !self.live.status.is_connected()
        {
            return Some("Disconnected. Drafts stay local; nothing is replayed.");
        }
        if self.navigation_fence.is_some() {
            return Some("Waiting for Herdr to confirm navigation.");
        }
        let (Some(target), Some(snapshot), Some(surface)) = (
            &self.composer_target,
            &self.live.snapshot,
            &self.live.surface,
        ) else {
            return Some("Waiting for a focused pane and its terminal surface.");
        };
        if surface.popup.is_some() {
            return Some("Popup active. Interact with the terminal before sending.");
        }
        if !self.input_ready()
            || !agent_mode::valid_target(
                target,
                &self.endpoints[self.selected_endpoint].id,
                snapshot,
                surface,
            )
        {
            return Some("Waiting for the selected pane's current surface.");
        }
        None
    }

    pub(super) fn set_tab_mode(
        &mut self,
        target: &TabTarget,
        mode: ViewMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if target.endpoint_id != self.endpoints[self.selected_endpoint].id {
            return;
        }
        let Some(snapshot) = &self.live.snapshot else {
            return;
        };
        if !self.agent_modes.set_mode(target, mode, snapshot) {
            return;
        }
        let active = snapshot.focused_tab_id.as_ref() == Some(&target.tab_id);
        self.sync_composer(cx);
        if active {
            self.invalidate_terminal_input();
            let focus = if mode == ViewMode::Agent && self.composer_editable() {
                self.composer.focus_handle(cx)
            } else {
                self.focus.clone()
            };
            window.focus(&focus);
        }
        cx.notify();
    }

    pub(super) fn restore_input_focus(
        &mut self,
        preferred: Option<FocusHandle>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_composer(cx);
        let composer = self.composer.focus_handle(cx);
        if preferred.as_ref() == Some(&composer) && self.agent_tab_active() {
            window.focus(&composer);
        } else {
            window.focus(&self.focus);
        }
    }

    pub(super) fn submit_composer(&mut self, cx: &mut Context<Self>) {
        let result = (|| -> Result<(), String> {
            if let Some(reason) = self.composer_block_reason() {
                return Err(reason.into());
            }
            let editor = self.composer.read(cx);
            if editor.is_composing() {
                return Err("Finish text composition before sending.".into());
            }
            let text = editor.text();
            if text.trim().is_empty() {
                return Err("Write a prompt before sending.".into());
            }
            let target = self
                .composer_target
                .as_ref()
                .ok_or("No composer recipient.")?;
            // Recheck the authoritative inbox, not only the last rendered clone.
            // try_lock and bounded enqueue cannot block the UI thread.
            let endpoint = &self.endpoints[self.selected_endpoint];
            let state = endpoint
                .connection
                .inbox
                .try_lock()
                .map_err(|_| "Herdr state is updating; draft was not sent.")?;
            let (Some(snapshot), Some(surface)) = (&state.snapshot, &state.surface) else {
                return Err("Herdr is not ready; draft was not sent.".into());
            };
            if !state.status.is_connected()
                || !state.surface_ready()
                || surface.frame.width != self.options.surface_size.cols
                || surface.frame.height != self.options.surface_size.rows
                || !agent_mode::valid_target(target, &endpoint.id, snapshot, surface)
            {
                return Err("The recipient or popup changed; draft was not sent.".into());
            }
            endpoint
                .connection
                .handle
                .as_ref()
                .ok_or("Disconnected; draft was not sent.")?
                .send_input(&target.boot_id, &target.pane_id, prompt_events(text))
                .map_err(|error| format!("Input not queued: {error}"))
        })();
        self.composer_notice = Some(match result {
            Ok(()) => {
                if let Some(target) = &self.composer_target {
                    self.drafts.remove(target);
                }
                self.composer.update(cx, |editor, cx| {
                    editor.set_draft(composer::Draft::default(), cx)
                });
                "Input queued, not confirmed. Check the terminal for acceptance.".into()
            }
            Err(error) => error,
        });
        cx.notify();
    }

    pub(super) fn render_composer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let recipient = self
            .composer_target
            .as_ref()
            .map(|target| {
                let name = self
                    .live
                    .snapshot
                    .as_ref()
                    .and_then(|snapshot| {
                        snapshot
                            .agents
                            .iter()
                            .find(|agent| agent.pane_id == target.pane_id)
                            .and_then(|agent| agent.name.as_deref().or(agent.agent.as_deref()))
                    })
                    .unwrap_or("Terminal");
                format!("Send to {} - {name}", target.pane_id)
            })
            .unwrap_or_else(|| "No focused pane".into());
        let blocked = self.composer_block_reason();
        let mode_target = self.live.snapshot.as_ref().and_then(|snapshot| {
            snapshot.focused_tab_id.as_ref().map(|tab| TabTarget {
                endpoint_id: self.endpoints[self.selected_endpoint].id.clone(),
                boot_id: snapshot.boot_id.clone(),
                tab_id: tab.clone(),
            })
        });
        let send_target = self.composer_target.clone();
        div().id("agent-composer").debug_selector(|| "agent-composer".into())
            .flex().flex_col().flex_none().min_w_0().border_t_1()
            .border_color(rgb(self.theme.active)).bg(rgb(self.theme.surface))
            .font_family(self.config.ui.family.clone()).text_size(px(self.config.ui.size))
            .px_3().py_2().gap_1().text_color(rgb(self.theme.foreground))
            .child(div().flex().items_center().gap_2().min_w_0()
                .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().child(recipient))
                .child(div().id("agent-terminal-mode").cursor_pointer().px_2().child("Terminal mode")
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(target) = &mode_target { this.set_tab_mode(target, ViewMode::Terminal, window, cx); }
                    })))
                .child(div().id("agent-send").debug_selector(|| "agent-send".into()).px_3().py_1().rounded_sm()
                    .bg(rgb(self.theme.active)).child("Send")
                    .when(blocked.is_none(), |button| button.cursor_pointer())
                    .when(blocked.is_some(), |button| button.opacity(0.45))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if this.composer_target == send_target {
                            this.submit_composer(cx);
                        }
                    }))))
            .child(self.composer.clone())
            .child(div().text_color(rgb(self.theme.muted)).child(
                blocked.map(str::to_owned).or_else(|| self.composer_notice.clone())
                    .unwrap_or_else(|| "Enter: newline | Cmd-Enter: send paste + Enter to the terminal. Use an empty agent prompt.".into())
            ))
    }
}

#[cfg(all(test, feature = "integration-test"))]
#[path = "agent_view/tests.rs"]
mod integration_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    #[test]
    fn submission_is_one_ordered_batch_with_multiline_paste_and_enter() {
        let events = prompt_events("first\nsecond\n");
        assert_eq!(
            events[0],
            ClientPaneInputEvent::Paste("first\nsecond\n".into())
        );
        assert!(matches!(
            events[1],
            ClientPaneInputEvent::Key {
                code: ClientKeyCode::Enter,
                modifiers: 0,
                kind: ClientKeyKind::Press,
                repeat_count: 1,
                ..
            }
        ));
    }
}
