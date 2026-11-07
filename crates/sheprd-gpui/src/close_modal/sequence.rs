//! Closing several tabs one after another, as Close Others does. Each tab
//! closes as its own close button would: a page at once, a Herdr tab through
//! its confirmation. The next terminal tab waits for the one before it to
//! be gone and the connection's surface barrier to settle, so two dialogs
//! never overlap, and a close the user declines ends the run
//! with the tabs that are left still open.

use crate::{
    SheprdWindow,
    browser::{Pick, Scope},
    menu::Page,
};
use gpui::{Context, Window};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

/// How long a close the daemon was sent may go unanswered before the run
/// gives up rather than wait on a tab that is not going away.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct CloseRun {
    workspace: (Scope, String),
    rest: VecDeque<Pick>,
    /// The Herdr tab whose close is being decided, or answered by the daemon.
    current: Option<String>,
    /// When its close went to the daemon.
    sent: Option<Instant>,
}

impl SheprdWindow {
    /// Closes `tabs` of the focused workspace in order.
    pub(crate) fn close_tabs(
        &mut self,
        tabs: Vec<Pick>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(workspace) = self.browser_key() else {
            return;
        };
        self.browser.closing = Some(CloseRun {
            workspace,
            rest: tabs.into(),
            current: None,
            sent: None,
        });
        self.advance_close_run(window, cx);
    }

    /// Records that the close of Herdr tab `tab` went to the daemon, so a run
    /// waiting on it knows the dialog was accepted rather than declined.
    pub(crate) fn close_sent(&mut self, tab: &str) {
        if let Some(run) = &mut self.browser.closing
            && run.current.as_deref() == Some(tab)
        {
            run.sent = Some(Instant::now());
        }
    }

    /// Whether the close confirmation is open for Herdr tab `tab`.
    fn confirming_tab(&self, tab: &str) -> bool {
        self.menu.page == Some(Page::ConfirmClose)
            && self
                .menu
                .close
                .as_ref()
                .is_some_and(|close| close.pane.is_none() && close.tab == tab)
    }

    /// Takes the run as far as it can go without the user or the daemon: on
    /// every tick, and when it starts.
    pub(crate) fn advance_close_run(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut run) = self.browser.closing.take() else {
            return;
        };
        // Leaving the workspace ends the run.
        if self.browser_key().as_ref() != Some(&run.workspace) {
            return;
        }
        loop {
            if let Some(tab) = run.current.clone() {
                let open = self.live.snapshot.as_ref().is_some_and(|snapshot| {
                    snapshot
                        .tabs
                        .iter()
                        .any(|candidate| candidate.tab_id == tab)
                });
                if !open {
                    run.current = None;
                    run.sent = None;
                    continue;
                }
                let answering = run.sent.is_some_and(|sent| sent.elapsed() < ANSWER_TIMEOUT);
                if self.confirming_tab(&tab) || answering {
                    break;
                }
                // Declined, or never answered: the rest stay open.
                return;
            }
            // Removing a tab from the snapshot does not acknowledge its
            // close's surface barrier. An immediate close would fail its
            // readiness check and leave an error dialog instead of waiting.
            if matches!(run.rest.front(), Some(Pick::Herdr(_))) && !self.input_ready() {
                break;
            }
            match run.rest.pop_front() {
                None => return,
                Some(Pick::Page(id)) => self.close_browser_tab(id, window, cx),
                Some(Pick::Herdr(tab)) => {
                    run.current = Some(tab.clone());
                    run.sent = None;
                    // In place while the close is presented, which may send
                    // it at once.
                    self.browser.closing = Some(run);
                    self.open_tab_close(&tab, window, cx);
                    let Some(presented) = self.browser.closing.take() else {
                        return;
                    };
                    run = presented;
                }
            }
        }
        self.browser.closing = Some(run);
    }
}
