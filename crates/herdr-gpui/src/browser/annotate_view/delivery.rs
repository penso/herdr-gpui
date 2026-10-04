//! Delivering a tab's notes to the agent that opened its page: straight to a
//! waiting `browser feedback`, pasted into its pane once it is idle, or kept
//! for `browser feedback` when it cannot be typed into.

use super::{
    super::{Feedback, Tab, annotate, feedback::Batch},
    screenshots::save_screenshots,
};
use crate::{HerdrWindow, connection::ConnectionBridge, terminal::InputTarget, window::Flash};
use gpui::{prelude::*, *};
use herdr_client::protocol::{
    AgentStatus, ClientKeyCode, ClientKeyKind, ClientPaneInputEvent, ClientShellSnapshot,
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// How long a batch waits for a busy agent before it is pasted anyway, or,
/// when the agent is asking a question, kept for `browser feedback`.
const HOLD: Duration = Duration::from_secs(120);
/// Lets the agent's input take the paste before Enter submits it.
const SUBMIT_DELAY: Duration = Duration::from_millis(150);

/// Notes on the way to an agent's pane, waiting for it to be idle.
pub(super) struct Delivery {
    pane_id: String,
    boot_id: String,
    text: String,
    until: Instant,
}

/// The agent in `pane_id` and what it is doing, if Herdr sees one there.
fn agent<'a>(
    snapshot: &'a ClientShellSnapshot,
    pane_id: &str,
) -> Option<&'a herdr_client::protocol::ClientShellAgent> {
    snapshot
        .agents
        .iter()
        .find(|agent| agent.pane_id == pane_id)
}

fn enter() -> ClientPaneInputEvent {
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
    }
}

impl HerdrWindow {
    /// The queued notes as a prompt, after their screenshots are saved to
    /// files the agent can read. Saving runs off the UI thread; `then` gets
    /// the prompt back on it.
    fn with_notes_prompt(
        &mut self,
        tab: &Tab,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, String, &mut Context<Self>) + 'static,
    ) {
        let notes = self.tab_notes(tab.id).notes.clone();
        if notes.is_empty() {
            return;
        }
        let reload = crate::control::reload_command();
        if notes.iter().all(|note| note.image.is_none()) {
            let text = annotate::prompt(tab, &notes, &[], &reload);
            then(self, text, cx);
            return;
        }
        let images: Vec<Option<Arc<Image>>> = notes.iter().map(|note| note.image.clone()).collect();
        let saving = cx
            .background_executor()
            .spawn(async move { save_screenshots(&images) });
        let tab = tab.clone();
        cx.spawn(async move |this, cx| {
            let paths = saving.await;
            this.update(cx, |this, cx| {
                let paths = paths.unwrap_or_else(|error| {
                    tracing::warn!(%error, "Could not save note screenshots");
                    this.show_flash(Flash::warning("Screenshots could not be saved"), cx);
                    Vec::new()
                });
                let text = annotate::prompt(&tab, &notes, &paths, &reload);
                then(this, text, cx);
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn copy_notes(&mut self, tab: &Tab, cx: &mut Context<Self>) {
        self.with_notes_prompt(tab, cx, |this, text, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            this.show_flash(Flash::success("Notes copied"), cx);
        });
    }

    /// Where Send delivers: the pane of the agent that opened the page,
    /// while this window shows that pane's daemon.
    fn origin_pane<'a>(&'a self, tab: &'a Tab) -> Option<(&'a str, &'a ClientShellSnapshot)> {
        let pane = tab.origin.as_deref()?;
        let snapshot = self.live.snapshot.as_deref()?;
        let here = super::super::view::scope(&self.endpoints[self.selected_endpoint]) == tab.scope;
        (here
            && snapshot
                .panes
                .iter()
                .any(|candidate| candidate.pane_id == pane))
        .then_some((pane, snapshot))
    }

    /// Sends the queued notes to the agent that opened the page: to it
    /// directly when it waits in `browser feedback`, otherwise into its pane
    /// once it is idle, and kept for `browser feedback` when its pane is gone.
    pub(in crate::browser) fn send_notes(&mut self, tab: &Tab, cx: &mut Context<Self>) {
        // The queue is cleared at once, so a second Send cannot repeat it
        // while screenshots are still being saved.
        let owned = tab.clone();
        self.with_notes_prompt(tab, cx, move |this, text, cx| {
            this.deliver_notes(&owned, text, cx);
        });
        self.clear_notes(tab.id, cx);
    }

    fn deliver_notes(&mut self, tab: &Tab, text: String, cx: &mut Context<Self>) {
        let Some(pane_id) = tab.origin.clone() else {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            self.show_flash(
                Flash::success("No agent opened this page, so the notes were copied"),
                cx,
            );
            return;
        };
        let waiting = cx
            .try_global::<Feedback>()
            .is_some_and(|feedback| feedback.is_waiting(&pane_id));
        let flash = if waiting {
            cx.default_global::<Feedback>()
                .keep(Batch { pane_id, text });
            Flash::success("Notes sent to the waiting agent")
        } else if let Some((pane, snapshot)) = self
            .origin_pane(tab)
            .filter(|(pane, snapshot)| agent(snapshot, pane).is_some())
        {
            let busy = agent(snapshot, pane).is_some_and(|agent| {
                matches!(
                    agent.agent_status,
                    AgentStatus::Working | AgentStatus::Blocked
                )
            });
            let delivery = Delivery {
                pane_id: pane.to_owned(),
                boot_id: snapshot.boot_id.clone(),
                text,
                until: Instant::now() + HOLD,
            };
            self.browser.annotations.deliveries.push(delivery);
            if busy {
                Flash::success("Notes will go to the agent once it is idle")
            } else {
                Flash::success("Notes sent to the agent")
            }
        } else if self.origin_pane(tab).is_some() {
            // A shell, not an agent: Enter there would run the notes.
            cx.default_global::<Feedback>()
                .keep(Batch { pane_id, text });
            Flash::warning("No agent runs in that pane; notes kept for `browser feedback`")
        } else {
            cx.default_global::<Feedback>()
                .keep(Batch { pane_id, text });
            Flash::warning("The agent's pane is not here; notes kept for `browser feedback`")
        };
        self.show_flash(flash, cx);
    }

    /// Pastes held notes into agents that became idle. Runs every tick.
    pub(crate) fn poll_deliveries(&mut self, cx: &mut Context<Self>) {
        if self.browser.annotations.deliveries.is_empty() {
            return;
        }
        let now = Instant::now();
        let deliveries = std::mem::take(&mut self.browser.annotations.deliveries);
        for delivery in deliveries {
            let waiting = cx
                .try_global::<Feedback>()
                .is_some_and(|feedback| feedback.is_waiting(&delivery.pane_id));
            let snapshot = self.live.snapshot.clone();
            let present = snapshot.as_deref().filter(|snapshot| {
                snapshot.boot_id == delivery.boot_id
                    && snapshot
                        .panes
                        .iter()
                        .any(|pane| pane.pane_id == delivery.pane_id)
            });
            let status = present
                .and_then(|snapshot| agent(snapshot, &delivery.pane_id))
                .map(|agent| agent.agent_status);
            let busy = matches!(status, Some(AgentStatus::Working | AgentStatus::Blocked));
            if present.is_some() && !waiting && busy && now < delivery.until {
                self.browser.annotations.deliveries.push(delivery);
                continue;
            }
            // Only a running agent's prompt is typed into. A pane whose agent
            // exited is a shell, where Enter would run the pasted text, page
            // quotes included; an agent asking the user something must not
            // have its answer typed by a paste. Both fetch the notes instead.
            let typable = matches!(
                status,
                Some(AgentStatus::Idle | AgentStatus::Done | AgentStatus::Working)
            );
            if waiting || present.is_none() || !typable {
                cx.default_global::<Feedback>().keep(Batch {
                    pane_id: delivery.pane_id,
                    text: delivery.text,
                });
                if !waiting {
                    self.show_flash(
                        Flash::warning("The agent is not ready; notes kept for `browser feedback`"),
                        cx,
                    );
                }
                continue;
            }
            self.paste_into_pane(delivery, cx);
        }
    }

    fn paste_into_pane(&mut self, delivery: Delivery, cx: &mut Context<Self>) {
        let target = InputTarget::Pane(delivery.pane_id.clone());
        let pasted = self.endpoints[self.selected_endpoint]
            .connection
            .handle
            .as_ref()
            .ok_or(crate::Error::NotConnected)
            .and_then(|handle| {
                ConnectionBridge::send_input(
                    handle,
                    &delivery.boot_id,
                    &target,
                    ClientPaneInputEvent::Paste(delivery.text.clone()),
                )
                .map_err(crate::Error::from)
            });
        if let Err(error) = pasted {
            tracing::warn!(%error, "Could not paste notes into the agent's pane");
            cx.default_global::<Feedback>().keep(Batch {
                pane_id: delivery.pane_id,
                text: delivery.text,
            });
            self.show_flash(
                Flash::warning("Could not reach the agent; notes kept for `browser feedback`"),
                cx,
            );
            return;
        }
        let timer = cx.background_executor().clone();
        let boot_id = delivery.boot_id;
        cx.spawn(async move |this, cx| {
            timer.timer(SUBMIT_DELAY).await;
            this.update(cx, |this, _| {
                let handle = this.endpoints[this.selected_endpoint]
                    .connection
                    .handle
                    .as_ref();
                if let Some(handle) = handle
                    && let Err(error) =
                        ConnectionBridge::send_input(handle, &boot_id, &target, enter())
                {
                    tracing::warn!(%error, "Could not submit notes in the agent's pane");
                }
            })
            .ok();
        })
        .detach();
    }
}
