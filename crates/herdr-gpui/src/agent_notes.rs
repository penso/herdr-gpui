//! Delivering notes the user wrote to the agent they are about, whether they
//! annotate a page the agent showed or review the changes it made. A batch
//! goes one way only: to the agent when it waits in `browser feedback
//! --wait`, otherwise pasted into its pane once it is idle, and kept for
//! `browser feedback` when its pane is gone or must not be typed into.
use crate::{
    HerdrWindow, browser::Feedback, connection::ConnectionBridge, terminal::InputTarget,
    window::Flash,
};
use gpui::{App, ClipboardItem, Context};
use herdr_client::protocol::{
    AgentStatus, ClientKeyCode, ClientKeyKind, ClientPaneInputEvent, ClientShellAgent,
    ClientShellSnapshot,
};
use std::{
    sync::{Arc, Weak},
    time::{Duration, Instant},
};

/// A tab cannot resend while its previous batch is saving, waiting for an
/// agent, or still held for feedback. The delivery owns the strong reference.
#[derive(Default)]
pub(crate) struct PendingSend(Weak<()>);

impl PendingSend {
    pub(crate) fn start(&mut self) -> Option<Arc<()>> {
        if self.0.strong_count() > 0 {
            return None;
        }
        let pending = Arc::new(());
        self.0 = Arc::downgrade(&pending);
        Some(pending)
    }
}

/// A daemon identity survives endpoint reordering but not session replacement.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Origin {
    endpoint: String,
    boot_id: String,
}

impl Origin {
    pub(crate) fn of(view: &HerdrWindow) -> Option<Self> {
        Some(Self {
            endpoint: view.endpoints.get(view.selected_endpoint)?.id.clone(),
            boot_id: view.live.snapshot.as_ref()?.boot_id.clone(),
        })
    }

    pub(crate) fn current(&self, view: &HerdrWindow) -> bool {
        view.endpoints
            .get(view.selected_endpoint)
            .is_some_and(|endpoint| endpoint.id == self.endpoint)
            && view
                .live
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.boot_id == self.boot_id)
    }
}

/// How long a batch waits for a busy agent before it is pasted anyway, or,
/// when the agent is asking a question, kept for `browser feedback`.
const HOLD: Duration = Duration::from_secs(120);
/// Lets the agent's input take the paste before Enter submits it.
const SUBMIT_DELAY: Duration = Duration::from_millis(150);

/// Notes on the way to an agent's pane, waiting for it to be idle.
struct Delivery {
    pane_id: String,
    origin: Origin,
    text: String,
    until: Instant,
    pending: Option<Arc<()>>,
}

/// The window's notes still waiting for their agents.
#[derive(Default)]
pub(crate) struct Deliveries(Vec<Delivery>);

impl Deliveries {
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }
}

/// The agent in `pane_id`, if Herdr sees one there.
pub(crate) fn agent<'a>(
    snapshot: &'a ClientShellSnapshot,
    pane_id: &str,
) -> Option<&'a ClientShellAgent> {
    snapshot
        .agents
        .iter()
        .find(|agent| agent.pane_id == pane_id)
}

/// Which of a list's notes Send delivers, given whether each was sent: the
/// ones not sent yet, or all of them again once every one was. Sent notes
/// stay listed, so an edit or a lost paste can be sent again.
pub(crate) fn round(sent: impl IntoIterator<Item = bool>) -> Vec<usize> {
    let sent: Vec<bool> = sent.into_iter().collect();
    let new: Vec<usize> = (0..sent.len()).filter(|&index| !sent[index]).collect();
    if new.is_empty() {
        (0..sent.len()).collect()
    } else {
        new
    }
}

/// What the Send button says for `total` notes, `unsent` of them not sent.
pub(crate) fn send_label(total: usize, unsent: usize) -> String {
    match unsent {
        0 => "Resend all".into(),
        _ if unsent == total => "Send to agent".into(),
        _ => format!("Send {unsent} new"),
    }
}

/// Notes as they may be typed into a pane. Each kind of note cleans the
/// untrusted text it quotes, but this is the one way out, so a control that
/// slipped through cannot end the bracketed paste early or press keys in the
/// agent's prompt. Line breaks are the only control kept.
fn typable(text: &str) -> String {
    text.chars()
        .filter(|c| *c == '\n' || !crate::notifications::unsafe_char(*c))
        .collect()
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

/// Only Unix has a control listener that can collect feedback. Elsewhere,
/// copying both preserves the notes and releases the pending-send token.
fn keep_or_copy(
    batch: crate::browser::Batch,
    pending: Option<Arc<()>>,
    reason: &str,
    cx: &mut App,
) -> Flash {
    if cfg!(unix) {
        cx.default_global::<Feedback>().keep(batch, pending);
        Flash::warning(format!("{reason}; notes kept for `browser feedback`"))
    } else {
        cx.write_to_clipboard(ClipboardItem::new_string(batch.text));
        Flash::warning(format!(
            "{reason}; notes copied (browser feedback is unavailable)"
        ))
    }
}

impl HerdrWindow {
    /// Sends `text` to the agent in `pane_id`. `here` says whether this
    /// window shows the daemon that pane belongs to; only then can it be
    /// typed into. `None` copies the notes, since no agent asked for them.
    pub(crate) fn deliver_notes(
        &mut self,
        pane_id: Option<String>,
        here: bool,
        text: String,
        pending: Option<Arc<()>>,
        cx: &mut Context<Self>,
    ) {
        let text = typable(&text);
        let Some(pane_id) = pane_id else {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            self.show_flash(
                Flash::success("No agent to send to, so the notes were copied"),
                cx,
            );
            return;
        };
        if !here {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            self.show_flash(
                Flash::warning("The original daemon is not selected; notes copied"),
                cx,
            );
            return;
        }
        let waiting = cfg!(unix)
            && cx
                .try_global::<Feedback>()
                .is_some_and(|feedback| feedback.is_waiting(&pane_id));
        let snapshot = self.live.snapshot.clone();
        let shown = snapshot.as_deref().filter(|snapshot| {
            here && snapshot
                .panes
                .iter()
                .any(|candidate| candidate.pane_id == pane_id)
        });
        let flash = if waiting {
            cx.default_global::<Feedback>()
                .keep(crate::browser::Batch { pane_id, text }, pending);
            Flash::success("Notes sent to the waiting agent")
        } else if let Some((snapshot, found)) =
            shown.and_then(|snapshot| Some((snapshot, agent(snapshot, &pane_id)?)))
        {
            let busy = matches!(
                found.agent_status,
                AgentStatus::Working | AgentStatus::Blocked
            );
            self.deliveries.0.push(Delivery {
                pane_id,
                origin: Origin {
                    endpoint: self.endpoints[self.selected_endpoint].id.clone(),
                    boot_id: snapshot.boot_id.clone(),
                },
                text,
                until: Instant::now() + HOLD,
                pending,
            });
            if busy {
                Flash::success("Notes will go to the agent once it is idle")
            } else {
                Flash::success("Notes sent to the agent")
            }
        } else if shown.is_some() {
            // A shell, not an agent: Enter there would run the notes.
            keep_or_copy(
                crate::browser::Batch { pane_id, text },
                pending,
                "No agent runs in that pane",
                cx,
            )
        } else {
            keep_or_copy(
                crate::browser::Batch { pane_id, text },
                pending,
                "The agent's pane is not here",
                cx,
            )
        };
        self.show_flash(flash, cx);
    }

    /// Pastes held notes into agents that became idle. Runs every tick.
    pub(crate) fn poll_deliveries(&mut self, cx: &mut Context<Self>) {
        if self.deliveries.0.is_empty() {
            return;
        }
        let now = Instant::now();
        let deliveries = std::mem::take(&mut self.deliveries.0);
        let mut stale = Vec::new();
        for delivery in deliveries {
            // Feedback only knows a pane ID, so it cannot safely hold a
            // batch after its endpoint or daemon has changed, even for a waiter.
            if !delivery.origin.current(self) {
                stale.push(delivery.text);
                continue;
            }
            let waiting = cfg!(unix)
                && cx
                    .try_global::<Feedback>()
                    .is_some_and(|feedback| feedback.is_waiting(&delivery.pane_id));
            let snapshot = self.live.snapshot.clone();
            let present = snapshot.as_deref().filter(|snapshot| {
                snapshot.boot_id == delivery.origin.boot_id
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
                self.deliveries.0.push(delivery);
                continue;
            }
            // Only a running agent's prompt is typed into. A pane whose agent
            // exited is a shell, where Enter would run the pasted text, quotes
            // included; an agent asking the user something must not have its
            // answer typed by a paste. Both fetch the notes instead.
            let typable = matches!(
                status,
                Some(AgentStatus::Idle | AgentStatus::Done | AgentStatus::Working)
            );
            if waiting || present.is_none() || !typable {
                let flash = keep_or_copy(
                    crate::browser::Batch {
                        pane_id: delivery.pane_id,
                        text: delivery.text,
                    },
                    delivery.pending,
                    "The agent is not ready",
                    cx,
                );
                if !waiting {
                    self.show_flash(flash, cx);
                }
                continue;
            }
            self.paste_into_pane(delivery, cx);
        }
        if !stale.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(stale.join("\n")));
            self.show_flash(
                Flash::warning("The original daemon is not selected; pending notes copied"),
                cx,
            );
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
                    &delivery.origin.boot_id,
                    &target,
                    ClientPaneInputEvent::Paste(delivery.text.clone()),
                )
                .map_err(crate::Error::from)
            });
        if let Err(error) = pasted {
            tracing::warn!(%error, "Could not paste notes into the agent's pane");
            let flash = keep_or_copy(
                crate::browser::Batch {
                    pane_id: delivery.pane_id,
                    text: delivery.text,
                },
                delivery.pending,
                "Could not reach the agent",
                cx,
            );
            self.show_flash(flash, cx);
            return;
        }
        let timer = cx.background_executor().clone();
        let origin = delivery.origin;
        let pending = delivery.pending;
        cx.spawn(async move |this, cx| {
            timer.timer(SUBMIT_DELAY).await;
            this.update(cx, |this, _| {
                if !origin.current(this) {
                    return;
                }
                let handle = this.endpoints[this.selected_endpoint]
                    .connection
                    .handle
                    .as_ref();
                if let Some(handle) = handle
                    && let Err(error) =
                        ConnectionBridge::send_input(handle, &origin.boot_id, &target, enter())
                {
                    tracing::warn!(%error, "Could not submit notes in the agent's pane");
                }
            })
            .ok();
            drop(pending);
        })
        .detach();
    }
}

#[cfg(test)]
mod tests;
