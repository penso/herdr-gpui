//! Bringing the agent that just took notes into view, so the user sees it
//! at work: its tab replaces a page in its group and its pane takes the
//! focus. It happens when the notes are handed over, pasted into the pane or
//! taken by the agent waiting in `browser feedback --wait`, not when a batch
//! is only queued for a busy agent, nor when it is kept or copied.

use super::Origin;
use crate::{SheprdWindow, browser::Pick, navigation::NavigationTarget};
use gpui::{Context, Window};
use std::time::{Duration, Instant};

/// How long a reveal waits for the window to accept a navigation, or for
/// the daemon to arrive in the agent's workspace, before it is dropped
/// rather than pulling the user back later.
const WAIT: Duration = Duration::from_secs(5);

/// An agent to bring into view on the next tick that can navigate.
pub(super) struct Reveal {
    pane_id: String,
    origin: Origin,
    until: Instant,
    /// Asked the daemon for the pane in another workspace already.
    navigated: bool,
}

impl SheprdWindow {
    /// Brings the agent in `pane_id` of `origin`'s session into view on a
    /// later tick, where the window is at hand. The latest request wins.
    pub(super) fn request_reveal(&mut self, origin: Origin, pane_id: &str) {
        self.deliveries.reveal = Some(Reveal {
            pane_id: pane_id.to_owned(),
            origin,
            until: Instant::now() + WAIT,
            navigated: false,
        });
    }

    /// Carries out a requested reveal. Runs every tick. The window comes
    /// forward too, for notes sent from a mockup window of their own.
    pub(crate) fn poll_reveal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut reveal) = self.deliveries.reveal.take() else {
            return;
        };
        // A reveal for another session, or one that waited too long, would
        // move a user who has gone on to something else.
        if Instant::now() >= reveal.until || !reveal.origin.current(self) {
            return;
        }
        let Some(snapshot) = self.live.snapshot.clone() else {
            return;
        };
        let Some(pane) = snapshot
            .panes
            .iter()
            .find(|pane| pane.pane_id == reveal.pane_id)
        else {
            return;
        };
        if self.menu.page.is_some() {
            self.deliveries.reveal = Some(reveal);
            return;
        }
        // Another workspace's groups are not loaded, and arriving there
        // keeps a page its group shows. Focusing the pane takes the daemon
        // there; the tab is chosen once it has arrived.
        if snapshot.focused_workspace_id.as_ref() != Some(&pane.workspace_id) {
            if !reveal.navigated && self.input_ready() {
                window.activate_window();
                reveal.navigated = self.navigate(NavigationTarget::Pane(&reveal.pane_id), cx);
            }
            self.deliveries.reveal = Some(reveal);
            return;
        }
        window.activate_window();
        if let Some(group) = self
            .ensure_layout()
            .map(|layout| layout.owner(&Pick::Herdr(pane.tab_id.clone())))
        {
            self.choose_herdr_tab(group, &pane.tab_id, window, cx);
        }
        if snapshot.focused_pane_id.as_deref() != Some(reveal.pane_id.as_str()) {
            let target = NavigationTarget::Pane(reveal.pane_id.as_str());
            if self.input_ready() {
                self.navigate(target, cx);
            } else {
                // Choosing the tab may have just fenced a focus change.
                self.pending_navigation = Some((&target).into());
            }
        }
        window.focus(&self.focus, cx);
        cx.notify();
    }
}
