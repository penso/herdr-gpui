//! Delivering notes the user wrote to the agent they are about, whether they
//! annotate a page the agent showed or review the changes it made. A batch
//! goes one way only: to the agent when it waits in `browser feedback
//! --wait`, otherwise pasted into its pane once it is idle, and kept for
//! `browser feedback` when its pane is gone or must not be typed into.
use crate::{
    HerdrWindow,
    browser::{Feedback, FeedbackKey},
    connection::ConnectionBridge,
    control::NotesTo,
    terminal::InputTarget,
    window::Flash,
};
use gpui::{App, ClipboardItem, Context};
use herdr_client::protocol::{
    AgentStatus, ClientKeyCode, ClientKeyKind, ClientPaneInputEvent, ClientShellAgent,
    ClientShellSnapshot,
};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::{Arc, Weak},
    time::{Duration, Instant},
};

/// Clipboard recovery for one terminal Send, shared by its agent batches so
/// a later failed paste still includes notes copied by an earlier fallback.
#[derive(Clone, Default)]
pub(crate) struct Copies(Rc<RefCell<Vec<String>>>);

impl Copies {
    pub(crate) fn extend(&self, texts: impl IntoIterator<Item = String>) {
        self.0.borrow_mut().extend(texts);
    }

    pub(crate) fn text(&self) -> Option<String> {
        let texts = self.0.borrow();
        (!texts.is_empty()).then(|| texts.join("\n"))
    }
}

fn collect_copy(collected: &mut Vec<Copies>, group: Option<Copies>, text: Option<String>) {
    let Some(text) = text else {
        return;
    };
    let group = group.unwrap_or_default();
    group.extend([text]);
    if !collected.iter().any(|other| Rc::ptr_eq(&other.0, &group.0)) {
        collected.push(group);
    }
}

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

/// Where `deliver_notes` put a batch.
#[must_use]
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Delivered {
    /// To the agent: now, once it is idle, or to it waiting in `browser
    /// feedback --wait`.
    Agent,
    /// Kept for `browser feedback`.
    Kept,
    /// Not delivered: the caller copies this text, together with any other
    /// batch of the same Send, in one clipboard write.
    Copy(String),
}

impl Delivered {
    /// The text left for the caller to copy.
    pub(crate) fn copy(self) -> Option<String> {
        match self {
            Self::Copy(text) => Some(text),
            Self::Agent | Self::Kept => None,
        }
    }
}

/// Notes on the way to an agent's pane, waiting for it to be idle.
struct Delivery {
    target: FeedbackKey,
    origin: Origin,
    text: String,
    until: Instant,
    pending: Option<Arc<()>>,
    copies: Option<Copies>,
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
/// return the text for the caller to copy together with other fallback batches.
fn keep_or_copy(
    batch: crate::browser::Batch,
    pending: Option<Arc<()>>,
    reason: &str,
    cx: &mut App,
) -> (Flash, Delivered) {
    if cfg!(unix) {
        cx.default_global::<Feedback>().keep(batch, pending);
        (
            Flash::warning(format!("{reason}; notes kept for `browser feedback`")),
            Delivered::Kept,
        )
    } else {
        (
            Flash::warning(format!(
                "{reason}; notes copied (browser feedback is unavailable)"
            )),
            Delivered::Copy(batch.text),
        )
    }
}

impl HerdrWindow {
    /// Sends `text` to the agent in the target pane, typing only into a pane
    /// of the daemon this window shows. `None` leaves the notes for the
    /// caller to copy, since no agent asked for them. Copied text comes back
    /// so the caller can combine every batch of one Send in one clipboard write.
    pub(crate) fn deliver_notes(
        &mut self,
        target: Option<FeedbackKey>,
        text: String,
        pending: Option<Arc<()>>,
        copies: Option<Copies>,
        cx: &mut Context<Self>,
    ) -> Delivered {
        let text = typable(&text);
        let Some(target) = target else {
            self.show_flash(
                Flash::success("No agent to send to, so the notes were copied"),
                cx,
            );
            return Delivered::Copy(text);
        };
        let waiting = cfg!(unix)
            && cx
                .try_global::<Feedback>()
                .is_some_and(|feedback| feedback.is_waiting(&target));
        let here = crate::browser::scope(&self.endpoints[self.selected_endpoint]) == target.scope;
        let snapshot = self.live.snapshot.clone();
        let shown = snapshot.as_deref().filter(|snapshot| {
            here && snapshot
                .panes
                .iter()
                .any(|candidate| candidate.pane_id == target.pane_id)
        });
        let (flash, delivered) = if waiting {
            cx.default_global::<Feedback>()
                .keep(crate::browser::Batch { target, text }, pending);
            (
                Flash::success("Notes sent to the waiting agent"),
                Delivered::Agent,
            )
        } else if let Some((snapshot, found)) =
            shown.and_then(|snapshot| Some((snapshot, agent(snapshot, &target.pane_id)?)))
        {
            let busy = matches!(
                found.agent_status,
                AgentStatus::Working | AgentStatus::Blocked
            );
            self.deliveries.0.push(Delivery {
                target,
                origin: Origin {
                    endpoint: self.endpoints[self.selected_endpoint].id.clone(),
                    boot_id: snapshot.boot_id.clone(),
                },
                text,
                until: Instant::now() + HOLD,
                pending,
                copies,
            });
            let flash = if busy {
                Flash::success("Notes will go to the agent once it is idle")
            } else {
                Flash::success("Notes sent to the agent")
            };
            (flash, Delivered::Agent)
        } else if shown.is_some() {
            // A shell, not an agent: Enter there would run the notes.
            keep_or_copy(
                crate::browser::Batch { target, text },
                pending,
                "No agent runs in that pane",
                cx,
            )
        } else {
            // Kept notes are keyed by their daemon, so they wait for the
            // agent there even when this window shows another one.
            keep_or_copy(
                crate::browser::Batch { target, text },
                pending,
                "The agent's pane is not here",
                cx,
            )
        };
        self.show_flash(flash, cx);
        delivered
    }

    /// Delivers notes written in `origin`'s session to the agent in `pane`
    /// there. Once that session is no longer shown, its pane ID may name
    /// another pane, so the notes come back to copy instead.
    pub(crate) fn deliver_from(
        &mut self,
        origin: Option<&Origin>,
        pane: Option<String>,
        text: String,
        pending: Option<Arc<()>>,
        cx: &mut Context<Self>,
    ) -> Delivered {
        let Some(pane_id) = pane else {
            return self.deliver_notes(None, text, pending, None, cx);
        };
        if !origin.is_some_and(|origin| origin.current(self)) {
            self.show_flash(
                Flash::warning("The agent's session is not shown here; notes copied"),
                cx,
            );
            return Delivered::Copy(typable(&text));
        }
        let scope = crate::browser::scope(&self.endpoints[self.selected_endpoint]);
        self.deliver_notes(
            Some(FeedbackKey { scope, pane_id }),
            text,
            pending,
            None,
            cx,
        )
    }

    /// Delivers notes a control request sent, if this window shows the
    /// caller's pane on its selected endpoint, the only one it types into.
    /// `None` leaves them to another window, or to the request to keep.
    #[cfg(unix)]
    pub(crate) fn deliver_requested_notes(
        &mut self,
        target: &crate::control::Target<'_>,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Option<NotesTo> {
        let pane = target.pane?;
        let ours = target.daemon.is_none_or(|daemon| {
            self.endpoints[self.selected_endpoint]
                .connection
                .target
                .socket_path()
                .ok()
                .as_deref()
                == Some(daemon)
        });
        let shown = self.live.snapshot.as_deref().is_some_and(|snapshot| {
            snapshot
                .panes
                .iter()
                .any(|candidate| candidate.pane_id == pane)
        });
        if !(ours && shown) {
            return None;
        }
        let target = FeedbackKey {
            scope: crate::browser::scope(&self.endpoints[self.selected_endpoint]),
            pane_id: pane.to_owned(),
        };
        match self.deliver_notes(Some(target), text.to_owned(), None, None, cx) {
            Delivered::Agent => Some(NotesTo::Agent),
            Delivered::Kept => Some(NotesTo::Kept),
            Delivered::Copy(_) => None,
        }
    }

    /// Pastes held notes into agents that became idle. Runs every tick.
    pub(crate) fn poll_deliveries(&mut self, cx: &mut Context<Self>) {
        if self.deliveries.0.is_empty() {
            return;
        }
        let now = Instant::now();
        let deliveries = std::mem::take(&mut self.deliveries.0);
        let mut copied = Vec::new();
        let mut stale = false;
        for delivery in deliveries {
            // Feedback is keyed by daemon, not session: once the session the
            // batch was written in is gone, its pane ID may name another
            // pane, so the notes are copied rather than kept or typed.
            if !delivery.origin.current(self) {
                collect_copy(&mut copied, delivery.copies, Some(delivery.text));
                stale = true;
                continue;
            }
            let waiting = cfg!(unix)
                && cx
                    .try_global::<Feedback>()
                    .is_some_and(|feedback| feedback.is_waiting(&delivery.target));
            let snapshot = self.live.snapshot.clone();
            let present = snapshot.as_deref().filter(|snapshot| {
                snapshot.boot_id == delivery.origin.boot_id
                    && snapshot
                        .panes
                        .iter()
                        .any(|pane| pane.pane_id == delivery.target.pane_id)
            });
            let status = present
                .and_then(|snapshot| agent(snapshot, &delivery.target.pane_id))
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
                let (flash, delivered) = keep_or_copy(
                    crate::browser::Batch {
                        target: delivery.target,
                        text: delivery.text,
                    },
                    delivery.pending,
                    "The agent is not ready",
                    cx,
                );
                collect_copy(&mut copied, delivery.copies, delivered.copy());
                if !waiting {
                    self.show_flash(flash, cx);
                }
                continue;
            }
            let group = delivery.copies.clone();
            collect_copy(&mut copied, group, self.paste_into_pane(delivery, cx));
        }
        if !copied.is_empty() {
            let text = copied
                .iter()
                .filter_map(Copies::text)
                .collect::<Vec<_>>()
                .join("\n");
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
        if stale {
            self.show_flash(
                Flash::warning("The original daemon is not selected; pending notes copied"),
                cx,
            );
        }
    }

    fn paste_into_pane(&mut self, delivery: Delivery, cx: &mut Context<Self>) -> Option<String> {
        let target = InputTarget::Pane(delivery.target.pane_id.clone());
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
            let (flash, delivered) = keep_or_copy(
                crate::browser::Batch {
                    target: delivery.target,
                    text: delivery.text,
                },
                delivery.pending,
                "Could not reach the agent",
                cx,
            );
            self.show_flash(flash, cx);
            return delivered.copy();
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
        None
    }
}

#[cfg(test)]
mod tests;
