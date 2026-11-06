//! Replying to agents from the Overview: a follow-up that waits until its
//! agent is back at its prompt, then goes through `herdr agent prompt`.
//!
//! The endpoint connection has no method to prompt an agent, only pane input,
//! and typed input cannot be taken back: a status read here can be stale by
//! the time it lands, so text and Enter could answer an approval the user
//! never saw. So a reply runs Herdr's own command on the agent's host, as a
//! pane move does, and the daemon refuses it with `agent_blocked`, in the same
//! step that would have sent it. A reply refused that way just waits again.
//!
//! The client follows the agent's status in its snapshots rather than holding
//! an `agent.wait` open per reply: they already arrive, for every host, and
//! the final say stays with the daemon. Replies live in this window's memory:
//! one queued when the app quits is not sent.

use super::{Error, Row};
use crate::{SheprdWindow, i18n::fl, teleport::Host, window::Flash};
use gpui::{Context, Window};
use sheprd_client::protocol::AgentStatus;
use std::{
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

/// How long a pane takes no further reply after one went in, so a second one
/// does not land before the agent shows it started on the first.
const SETTLE: Duration = Duration::from_secs(5);
/// Replies held at once, across every agent.
pub(super) const MAX_QUEUED: usize = 64;
/// Bytes of a reply: it travels as one shell argument.
pub(crate) const MAX_TEXT: usize = 4096;
/// The daemon's refusal of a prompt to an agent that is asking for input.
const BLOCKED: &str = "agent_blocked";

/// Where a reply stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Waiting,
    /// On its way; a reply here can no longer be cancelled.
    Sending,
    /// Why it did not go, shown until the user dismisses it.
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Reply {
    pub id: u64,
    pub endpoint: String,
    pub pane: String,
    /// The boot the pane ID belongs to.
    boot: String,
    /// The agent the reply was written to, as a pane can change hands.
    agent: String,
    pub text: String,
    pub phase: Phase,
}

/// What a reply's pane holds now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Presence {
    /// The host is not connected, so nothing is known.
    Unreachable,
    /// The agent, pane, or daemon boot the reply was written to is gone.
    Gone,
    Here(AgentStatus),
}

/// What one tick found to do.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Advance {
    /// Replies now marked as sending, for the caller to send.
    pub send: Vec<Reply>,
    /// Replies whose agent is gone, already taken off the queue.
    pub dropped: Vec<Reply>,
}

#[derive(Debug, Default)]
pub(crate) struct Replies {
    queue: Vec<Reply>,
    next: u64,
    /// Panes that took a reply lately, by host, with when they take another.
    settling: Vec<(String, String, Instant)>,
}

impl Replies {
    pub(crate) fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub(crate) fn of<'a>(
        &'a self,
        endpoint: &'a str,
        pane: &'a str,
    ) -> impl Iterator<Item = &'a Reply> {
        self.queue
            .iter()
            .filter(move |reply| reply.endpoint == endpoint && reply.pane == pane)
    }

    /// Queues `text` for an agent. False when the queue is full.
    pub(super) fn push(
        &mut self,
        (endpoint, pane): (&str, &str),
        boot: &str,
        agent: &str,
        text: &str,
    ) -> bool {
        if self.queue.len() >= MAX_QUEUED {
            return false;
        }
        self.queue.push(Reply {
            id: self.next,
            endpoint: endpoint.to_owned(),
            pane: pane.to_owned(),
            boot: boot.to_owned(),
            agent: agent.to_owned(),
            text: text.to_owned(),
            phase: Phase::Waiting,
        });
        self.next += 1;
        true
    }

    /// Takes a reply back. One on its way cannot be.
    pub(super) fn cancel(&mut self, id: u64) -> bool {
        let before = self.queue.len();
        self.queue
            .retain(|reply| reply.id != id || reply.phase == Phase::Sending);
        self.queue.len() != before
    }

    /// Marks the replies that can go now, in order: a pane takes its oldest
    /// first, one at a time, and only at its prompt.
    pub(super) fn advance(
        &mut self,
        now: Instant,
        presence: impl Fn(&Reply) -> Presence,
    ) -> Advance {
        self.settling.retain(|(_, _, until)| now < *until);
        let mut advance = Advance::default();
        let mut taken: Vec<(String, String)> = Vec::new();
        let settling = std::mem::take(&mut self.settling);
        self.queue.retain_mut(|reply| {
            let pane = (reply.endpoint.clone(), reply.pane.clone());
            if reply.phase == Phase::Sending {
                taken.push(pane);
                return true;
            }
            if reply.phase != Phase::Waiting {
                return true;
            }
            let status = match presence(reply) {
                Presence::Unreachable => {
                    taken.push(pane);
                    return true;
                }
                Presence::Gone => {
                    advance.dropped.push(reply.clone());
                    return false;
                }
                Presence::Here(status) => status,
            };
            let free = !taken.contains(&pane)
                && !settling
                    .iter()
                    .any(|(endpoint, id, _)| *endpoint == pane.0 && *id == pane.1);
            if free && matches!(status, AgentStatus::Idle | AgentStatus::Done) {
                reply.phase = Phase::Sending;
                advance.send.push(reply.clone());
            }
            taken.push(pane);
            true
        });
        self.settling = settling;
        advance
    }

    /// Takes in how a send ended.
    pub(super) fn finish(&mut self, id: u64, result: Result<(), Error>, now: Instant) {
        let Some(index) = self.queue.iter().position(|reply| reply.id == id) else {
            return;
        };
        match result {
            Ok(()) => {
                let reply = self.queue.remove(index);
                self.settling
                    .push((reply.endpoint, reply.pane, now + SETTLE));
            }
            // Nothing reached the pane: the agent asked for input first.
            Err(Error::ReplyRefused { code, .. }) if code == BLOCKED => {
                self.queue[index].phase = Phase::Waiting;
            }
            Err(error) => {
                self.queue[index].phase =
                    Phase::Failed(crate::i18n::errors::Localize::localize(&error));
            }
        }
    }
}

/// `text` as the one argument `herdr agent prompt` takes. Blocking: call only
/// on a background worker.
fn deliver(host: &Host, pane: &str, text: &str) -> Result<(), Error> {
    let line = Host::herdr_line(&["agent", "prompt", pane, text]);
    host.capture(&line, &AtomicBool::new(false))
        .map(|_| ())
        .map_err(|error| match crate::pane_move::refused(&error) {
            Some((code, message)) => Error::ReplyRefused { code, message },
            None => Error::ReplyScript(error),
        })
}

impl SheprdWindow {
    /// What `reply`'s pane holds, from the prepared snapshots.
    fn reply_presence(&self, reply: &Reply) -> Presence {
        let Some((index, endpoint)) = self
            .endpoints
            .iter()
            .enumerate()
            .find(|(_, endpoint)| endpoint.id == reply.endpoint)
        else {
            return Presence::Gone;
        };
        let live = if index == self.selected_endpoint {
            &self.live
        } else {
            &endpoint.live
        };
        let Some(snapshot) = live
            .snapshot
            .as_deref()
            .filter(|_| endpoint.enabled && live.status.is_connected())
        else {
            return Presence::Unreachable;
        };
        if snapshot.boot_id != reply.boot {
            return Presence::Gone;
        }
        snapshot
            .agents
            .iter()
            .find(|agent| agent.pane_id == reply.pane)
            .filter(|agent| crate::sidebar::agent_name(agent) == reply.agent)
            .map_or(Presence::Gone, |agent| Presence::Here(agent.agent_status))
    }

    /// Sends the replies whose agents are at their prompts, on every tick.
    pub(super) fn poll_overview_replies(&mut self, cx: &mut Context<Self>) {
        if self.overview.replies.is_empty() {
            return;
        }
        let mut replies = std::mem::take(&mut self.overview.replies);
        let advance = replies.advance(Instant::now(), |reply| self.reply_presence(reply));
        self.overview.replies = replies;
        if advance.send.is_empty() && advance.dropped.is_empty() {
            return;
        }
        for reply in advance.send {
            self.send_overview_reply(reply, cx);
        }
        if let Some(reply) = advance.dropped.first() {
            let text = crate::notifications::safe_text(&reply.text, 60);
            self.show_flash(
                Flash::warning(fl!("overview-reply-dropped", text = text)),
                cx,
            );
        }
        cx.notify();
    }

    fn send_overview_reply(&mut self, reply: Reply, cx: &mut Context<Self>) {
        let host = self
            .endpoints
            .iter()
            .find(|endpoint| endpoint.id == reply.endpoint)
            .ok_or(crate::pane_move::Error::UnsupportedHost)
            .and_then(|endpoint| crate::pane_move::host_for(&endpoint.connection.target));
        let id = reply.id;
        let work = cx.background_executor().spawn(async move {
            deliver(&host.map_err(Error::ReplyHost)?, &reply.pane, &reply.text)
        });
        cx.spawn(async move |this, cx| {
            let result = work.await;
            this.update(cx, |this, cx| {
                this.overview.replies.finish(id, result, Instant::now());
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Opens the reply dialog for `rows`, leaving out what cannot take one.
    pub(super) fn open_overview_reply(
        &mut self,
        rows: Vec<Row>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let total = rows.len();
        let rows: Vec<Row> = rows
            .into_iter()
            .filter(|row| self.overview_row_takes_replies(row))
            .collect();
        if rows.is_empty() {
            self.show_flash(Flash::warning(fl!("overview-reply-unsupported")), cx);
            return;
        }
        if rows.len() < total {
            let skipped = total - rows.len();
            self.show_flash(
                Flash::warning(fl!("overview-reply-skipped", count = skipped)),
                cx,
            );
        }
        if self.open_menu(window, cx) {
            self.overview.reply_to = rows;
            crate::menu::open_overview_reply(self);
        }
    }

    /// Whether `row` is an agent on a host whose `herdr` CLI this client can
    /// run.
    fn overview_row_takes_replies(&self, row: &Row) -> bool {
        row.pane().is_some_and(|(id, _)| {
            self.endpoints.iter().any(|endpoint| {
                endpoint.id == id
                    && crate::pane_move::refusal(&endpoint.connection.target).is_none()
            })
        })
    }

    /// The rows the reply dialog is for.
    pub(crate) fn overview_reply_targets(&self) -> &[Row] {
        &self.overview.reply_to
    }

    /// Queues `text` for each row the dialog was opened for.
    pub(crate) fn queue_overview_replies(&mut self, text: &str, cx: &mut Context<Self>) {
        let rows = std::mem::take(&mut self.overview.reply_to);
        let text = text.trim();
        let mut unqueued = 0;
        for row in &rows {
            let queued = row.pane().and_then(|pane| {
                let (index, endpoint) = self
                    .endpoints
                    .iter()
                    .enumerate()
                    .find(|(_, endpoint)| endpoint.id == pane.0)?;
                let live = if index == self.selected_endpoint {
                    &self.live
                } else {
                    &endpoint.live
                };
                let snapshot = live.snapshot.as_deref()?;
                let agent = snapshot.agents.iter().find(|a| a.pane_id == pane.1)?;
                let name = crate::sidebar::agent_name(agent);
                let boot = &snapshot.boot_id;
                Some(self.overview.replies.push(pane, boot, name, text))
            });
            unqueued += usize::from(queued != Some(true));
        }
        if unqueued > 0 {
            self.show_flash(
                Flash::warning(fl!("overview-reply-not-queued", count = unqueued)),
                cx,
            );
        }
        self.overview.selection.clear();
        self.poll_overview_replies(cx);
        cx.notify();
    }
}
