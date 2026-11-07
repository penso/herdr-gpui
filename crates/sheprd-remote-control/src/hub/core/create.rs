//! New work from a phone. While a core answers, new tabs and chats start
//! here and never on Herdr 1: a failure reaches the phone as it is, rather
//! than quietly landing the work on the other generation.
//!
//! A chat is `sheprd-acp` as the terminal's program, as the desktop starts
//! one: the core keeps no pane metadata a program could report through, so
//! the endpoint is chosen here and kept as a label.

use super::{Error, HubError, Result, Source, lock, pane_id, worktree};
use crate::hub::Layout;
use serde_json::{Value, json};
use sheprd_chat_protocol::{Agent, ENDPOINT_ENV, EndpointId, TOKEN};
use sheprd_core_client::wire::Run;
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};

/// The variable naming the Herdr pane `sheprd-acp` reports to. A core
/// started from a Herdr pane passes that pane on; a chat must not report
/// itself there.
const PANE_ENV: &str = "HERDR_PANE_ID";

impl Source {
    /// A shell in `cwd`, or the core's default folder when it is empty.
    pub(in crate::hub) fn open_shell(&self, cwd: &str) -> Result<String> {
        self.run(&Run {
            cwd: Some(cwd).filter(|cwd| !cwd.is_empty()),
            keep: true,
            ..Run::default()
        })
    }

    /// A chat running `agent` in `cwd`, listed once the next poll sees it.
    pub(in crate::hub) fn open_chat(
        &self,
        agent: Agent,
        program: &Path,
        cwd: &str,
    ) -> Result<String> {
        let endpoint = EndpointId::random().to_string();
        let argv = [program.to_string_lossy().into_owned(), agent.to_string()];
        self.run(&Run {
            argv: &argv,
            cwd: Some(cwd).filter(|cwd| !cwd.is_empty()),
            env: BTreeMap::from([(ENDPOINT_ENV, endpoint.as_str()), (PANE_ENV, "")]),
            // A recovery shell cannot serve the labelled chat endpoint.
            // Let a lost chat finish instead of advertising a dead chat.
            keep: false,
            labels: BTreeMap::from([(TOKEN, endpoint.as_str())]),
            ..Run::default()
        })
    }

    /// A chat as [`Self::open_chat`], in a new worktree of `cwd` on `branch`
    /// under `root`.
    pub(in crate::hub) fn open_worktree_chat(
        &self,
        agent: Agent,
        program: &Path,
        cwd: &str,
        branch: &str,
        root: Option<&Path>,
    ) -> Result<String> {
        let root = root.ok_or(worktree::Error::NoHome).map_err(Error::from)?;
        let repository = Path::new(cwd);
        let checkout = worktree::add(repository, branch, root).map_err(Error::from)?;
        let started = self.open_chat(agent, program, &checkout.to_string_lossy());
        if let Err(error) = &started {
            // A retry must find the branch free, but only a start that
            // surely ran nothing may go: a chat may be working in the
            // checkout after any other failure.
            if !ran_nothing(error) {
                tracing::warn!(%error, %branch, "A worktree stayed: its chat may have started");
            } else if let Err(error) = worktree::remove(repository, &checkout, branch) {
                tracing::warn!(%error, %branch, "A worktree without a chat stayed");
            }
        }
        started
    }

    fn run(&self, run: &Run<'_>) -> Result<String> {
        let core = lock(&self.state).core.clone();
        let core = core.ok_or(Error::Disconnected)?;
        let terminal = core.run(run)?;
        let pane = pane_id(&core.hello().core.id, &terminal.address);
        // Input may arrive before the next poll lists it.
        lock(&self.state).terminals.push(terminal);
        Ok(pane)
    }

    /// Ends the one terminal `target` names: a pane, or a tab holding only
    /// that pane. A tab of several is refused, so nothing closes in bulk.
    pub(super) fn close(&self, target: &str) -> Result<Value> {
        let (core, address) = {
            let state = lock(&self.state);
            let core = state.core.clone().ok_or(Error::Disconnected)?;
            let pane = match state.tabs.get(target).map(Vec::as_slice) {
                Some([pane]) => pane.as_str(),
                Some(_) => return Err(Error::Unsupported.into()),
                None => target,
            };
            let address = state
                .terminals
                .iter()
                .map(|terminal| terminal.address.clone())
                .find(|address| pane_id(&core.hello().core.id, address) == pane)
                .ok_or(Error::Unsupported)?;
            (core, address)
        };
        core.kill(&address)?;
        let mut state = lock(&self.state);
        state.screens.remove(&address);
        state
            .terminals
            .retain(|terminal| terminal.address != address);
        Ok(json!({}))
    }
}

/// Whether a failed start surely started no terminal: the core refused it,
/// or there was no core to ask. A timeout or lost connection may come after
/// the core acted on the request.
pub(super) fn ran_nothing(error: &HubError) -> bool {
    matches!(
        error,
        HubError::Core(
            Error::Disconnected | Error::Client(sheprd_core_client::Error::Refused { .. })
        )
    )
}

/// The panes of each tab in `layout`. A chat is listed apart from its tab's
/// terminals, so a tab of a chat and a shell still counts as two.
pub(super) fn tab_panes(layout: &Layout) -> HashMap<String, Vec<String>> {
    let mut tabs: HashMap<String, Vec<String>> = HashMap::new();
    for workspace in &layout.workspaces {
        for terminal in &workspace.terminals {
            let panes = terminal.panes.iter().map(|pane| pane.pane.clone());
            tabs.entry(terminal.tab.clone()).or_default().extend(panes);
        }
        for chat in &workspace.chats {
            let panes = tabs.entry(chat.tab.clone()).or_default();
            panes.push(chat.pane.clone());
        }
    }
    tabs
}
