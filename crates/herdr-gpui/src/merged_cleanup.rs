//! Removing a linked worktree, and closing its workspace, once its pull request
//! is merged. Opt-in through `remove_merged_worktrees`, because it deletes a
//! checkout without asking; the focused workspace still asks, through the
//! same dialog its menu's "Delete worktree checkout" opens.
//!
//! Only this machine's daemon is cleaned up, since the checkout is verified
//! with local Git before anything is sent. The request is `worktree.remove`
//! without force, so the daemon itself refuses a checkout that gained
//! uncommitted files after the check; nothing here ever retries with force.

use crate::{
    HerdrWindow, menu,
    pull_request::{Input, Origin, PullRequest, State, local_checkout, workspace_input},
    worktree_scripts::ScriptKind,
};
use gpui::{Context, Window};
use herdr_client::{
    ConnectTarget,
    protocol::{AgentStatus, ClientShellSnapshot, ClientShellWorkspace},
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

/// How often the cache is scanned for a merged pull request.
const SCAN: Duration = Duration::from_secs(1);
/// How long asking waits for an open menu or dialog before checking again.
const ASK_DEFER: Duration = Duration::from_secs(5);
/// How long a candidate that was not removable waits before it is checked
/// again: uncommitted files may be committed, a busy agent may finish.
const RETRY: Duration = Duration::from_secs(300);
/// Candidates remembered at once, so a long session cannot grow the ledger.
const LEDGER_LIMIT: usize = 128;
/// The local Git and file reads of one check.
const PROBE_DEADLINE: Duration = Duration::from_secs(15);

/// One merged pull request on one workspace's checkout. The PR's head commit
/// is part of the identity, so a branch reused after a merge is a new one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub(crate) workspace: String,
    pub(crate) input: Input,
    pub(crate) number: u64,
    pub(crate) head: String,
}

/// What the local check found in the checkout.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Report {
    /// The checkout's `HEAD` commit.
    pub(crate) head: String,
    /// Modified, staged, or untracked files, submodules included.
    pub(crate) dirty: bool,
    /// The repository runs an archive script before a checkout is removed.
    pub(crate) archive: bool,
}

/// Why a merged candidate is left alone for now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Skip {
    /// Uncommitted or untracked files are in the checkout.
    Dirty,
    /// The checkout is not at the merged head: commits came after it, or the
    /// branch was reused, so the merge does not cover what is checked out.
    HeadMoved,
    /// An agent in the workspace is working or waiting on a prompt.
    AgentBusy,
    /// The archive script runs in a tab after a trust prompt, which only the
    /// dialog can offer.
    ArchiveScript,
}

/// What to do with a candidate whose check passed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    /// Send `worktree.remove` without asking.
    Remove,
    /// The user is in this workspace: ask with the delete dialog instead.
    Ask,
}

/// Decide a checked candidate. Pure, so every refusal is testable.
pub(crate) fn decide(
    candidate: &Candidate,
    report: &Report,
    workspace: &ClientShellWorkspace,
    focused: bool,
) -> Result<Action, Skip> {
    if report.dirty {
        return Err(Skip::Dirty);
    }
    if candidate.head.is_empty() || !report.head.eq_ignore_ascii_case(&candidate.head) {
        return Err(Skip::HeadMoved);
    }
    if matches!(
        workspace.agent_status,
        AgentStatus::Working | AgentStatus::Blocked
    ) {
        return Err(Skip::AgentBusy);
    }
    if focused {
        // The dialog reads and offers the archive script itself.
        return Ok(Action::Ask);
    }
    if report.archive {
        return Err(Skip::ArchiveScript);
    }
    Ok(Action::Remove)
}

/// The workspaces of `snapshot` whose cached pull request is merged, in
/// snapshot order. Only linked worktrees qualify, never a main checkout.
pub(crate) fn candidates<'a, R: std::ops::Deref<Target = PullRequest>>(
    snapshot: &'a ClientShellSnapshot,
    pull_request: impl Fn(&Input) -> Option<R> + 'a,
) -> impl Iterator<Item = Candidate> + 'a {
    snapshot.workspaces.iter().filter_map(move |workspace| {
        if !workspace
            .worktree
            .as_ref()
            .is_some_and(|tree| tree.is_linked_worktree)
        {
            return None;
        }
        let input = workspace_input(workspace, &Origin::Local).ok()?;
        let pr = pull_request(&input).filter(|pr| pr.state == State::Merged)?;
        Some(Candidate {
            workspace: workspace.workspace_id.clone(),
            number: pr.number,
            head: pr.head_ref_oid.clone(),
            input,
        })
    })
}

/// Candidates already handled: `None` never again this session (removed or
/// asked), `Some(time)` not before then.
#[derive(Default)]
pub(crate) struct Ledger {
    entries: Vec<(Candidate, Option<Instant>)>,
}

impl Ledger {
    pub(crate) fn waiting(&self, candidate: &Candidate, now: Instant) -> bool {
        self.entries
            .iter()
            .any(|(old, until)| old == candidate && until.is_none_or(|until| now < until))
    }

    pub(crate) fn settle(&mut self, candidate: Candidate, until: Option<Instant>) {
        self.entries.retain(|(old, _)| old != &candidate);
        if self.entries.len() == LEDGER_LIMIT {
            self.entries.remove(0);
        }
        self.entries.push((candidate, until));
    }
}

/// A check running on the background executor. Dropping it cancels the read.
struct Probe {
    candidate: Candidate,
    receiver: mpsc::Receiver<crate::Result<Report>>,
    cancelled: Arc<AtomicBool>,
}

impl Drop for Probe {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

/// The window's automatic cleanup, fenced like a menu action by selection
/// epoch, connection generation, and daemon boot.
#[derive(Default)]
pub(crate) struct Cleanup {
    scope: Option<(u64, u64, String)>,
    probe: Option<Probe>,
    /// The removal sent for a candidate, by request ID, until it is answered.
    removing: Option<(Candidate, String)>,
    ledger: Ledger,
    next_scan: Option<Instant>,
}

/// Read the checkout's head, status, and archive script. Blocking: call only
/// on the background executor.
fn probe(input: &Input, cancelled: &AtomicBool) -> crate::Result<Report> {
    let deadline = Instant::now() + PROBE_DEADLINE;
    let stop = || cancelled.load(Ordering::Relaxed);
    let checkout = local_checkout(input, deadline, &stop)?;
    let head = crate::git::git(
        &checkout,
        &["rev-parse", "--verify", "HEAD"],
        "read HEAD",
        deadline,
        &stop,
    )?;
    let dirty = !crate::git::git(
        &checkout,
        &[
            "status",
            "--porcelain=v1",
            "--untracked-files=normal",
            "--ignore-submodules=none",
        ],
        "check uncommitted files",
        deadline,
        &stop,
    )?
    .is_empty();
    let archive =
        crate::worktree_scripts::read_config(&ConnectTarget::Local, &checkout, cancelled)?
            .is_some_and(|config| config.scripts.get(ScriptKind::Archive).is_some());
    Ok(Report {
        head,
        dirty,
        archive,
    })
}

impl HerdrWindow {
    /// A removal the user asked for, still waiting on the daemon. Cleanup
    /// waits for it, so the daemon is never sent two removals at once.
    fn removal_pending(&self) -> bool {
        self.removal.as_ref().is_some_and(menu::Removal::waiting)
    }

    /// Scan for a merged pull request, check its checkout off the UI thread,
    /// and remove it or ask. One check and one removal at a time.
    pub(crate) fn update_merged_cleanup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.config.remove_merged_worktrees {
            if self.merged_cleanup.scope.is_some() {
                self.merged_cleanup = Cleanup::default();
            }
            return;
        }
        let scope = self.live.snapshot.as_ref().map(|snapshot| {
            (
                self.selection_epoch,
                self.endpoints[self.selected_endpoint].generation,
                snapshot.boot_id.clone(),
            )
        });
        let active = self.live.status.is_connected()
            && self.pr_origin() == Some(Origin::Local)
            && scope.is_some();
        if !active || self.merged_cleanup.scope != scope {
            // The ledger is per daemon boot: workspace IDs do not outlive it,
            // and a reply from a replaced connection is not this removal's.
            self.merged_cleanup = Cleanup {
                scope: scope.filter(|_| active),
                ..Cleanup::default()
            };
            if !active {
                return;
            }
        }
        if self.merged_cleanup.removing.is_some() {
            self.finish_merged_removal(cx);
            return;
        }
        if self.merged_cleanup.probe.is_some() {
            self.finish_merged_probe(window, cx);
            return;
        }
        let now = Instant::now();
        if self.removal_pending() || self.merged_cleanup.next_scan.is_some_and(|next| now < next) {
            return;
        }
        self.merged_cleanup.next_scan = Some(now + SCAN);
        let Some(snapshot) = &self.live.snapshot else {
            return;
        };
        let cache = &self.menu.pr_cache;
        let ledger = &self.merged_cleanup.ledger;
        let Some(candidate) = candidates(snapshot, |input| cache.get(input))
            .find(|candidate| !ledger.waiting(candidate, now))
        else {
            return;
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let (input, cancellation) = (candidate.input.clone(), cancelled.clone());
        cx.background_executor()
            .spawn(async move {
                let _ = sender.send(probe(&input, &cancellation));
            })
            .detach();
        self.merged_cleanup.probe = Some(Probe {
            candidate,
            receiver,
            cancelled,
        });
    }

    fn finish_merged_probe(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = match self
            .merged_cleanup
            .probe
            .as_ref()
            .map(|p| p.receiver.try_recv())
        {
            Some(Ok(result)) => result,
            Some(Err(mpsc::TryRecvError::Empty)) | None => return,
            Some(Err(mpsc::TryRecvError::Disconnected)) => Err(crate::Error::StaleWorkspace),
        };
        let Some(probe) = self.merged_cleanup.probe.take() else {
            return;
        };
        let candidate = probe.candidate.clone();
        let now = Instant::now();
        let retry = Some(now + RETRY);
        // Recheck against what is live now: the workspace may have changed
        // branch, closed, or gained focus while Git ran.
        let Some(snapshot) = self.live.snapshot.clone() else {
            return;
        };
        let cache = &self.menu.pr_cache;
        let current = candidates(&snapshot, |input| cache.get(input)).any(|c| c == candidate);
        let workspace = snapshot
            .workspaces
            .iter()
            .find(|workspace| workspace.workspace_id == candidate.workspace);
        let (Some(workspace), true) = (workspace, current) else {
            return;
        };
        let report = match result {
            Ok(report) => report,
            // Git's diagnostics may name private paths; log the category only.
            Err(_) => {
                tracing::debug!(category = "check_failed", "Merged worktree kept");
                self.merged_cleanup.ledger.settle(candidate, retry);
                return;
            }
        };
        // Focus the user asked for is queued before the snapshot shows it, so
        // the focused workspace is known only once navigation has settled. A
        // workspace whose menu is open is in use too.
        if self.live.activation_pending()
            || self.menu.open_on(&candidate.workspace)
            || self.removal_pending()
        {
            self.merged_cleanup.next_scan = Some(now + ASK_DEFER);
            return;
        }
        let focused = snapshot.focused_workspace_id.as_deref() == Some(&candidate.workspace);
        match decide(&candidate, &report, workspace, focused) {
            Err(skip) => {
                tracing::debug!(?skip, "Merged worktree kept");
                self.merged_cleanup.ledger.settle(candidate, retry);
            }
            Ok(Action::Ask) => {
                // Never interrupt an open menu or dialog; ask once it closes.
                if self.menu.page.is_some() {
                    self.merged_cleanup.next_scan = Some(now + ASK_DEFER);
                    return;
                }
                self.ask_merged_removal(&candidate.workspace, candidate.number, window, cx);
                self.merged_cleanup.ledger.settle(candidate, None);
            }
            Ok(Action::Remove) => {
                let request = self.endpoints[self.selected_endpoint]
                    .connection
                    .request_cleanup(
                        &snapshot.boot_id,
                        herdr_client::Method::WorktreeRemove,
                        serde_json::json!({"workspace_id": candidate.workspace,
                            "force": false, "trust_repository": false}),
                    );
                match request {
                    Ok(id) => self.merged_cleanup.removing = Some((candidate, id)),
                    Err(_) => {
                        tracing::warn!(category = "not_queued", "Merged worktree kept");
                        self.merged_cleanup.ledger.settle(candidate, retry);
                    }
                }
            }
        }
        cx.notify();
    }

    /// Apply the daemon's answer to the cleanup's removal. Only success
    /// settles the candidate for good; a refusal, such as files that appeared
    /// after the check, is retried later and never escalated to force.
    fn finish_merged_removal(&mut self, cx: &mut Context<Self>) {
        let Some((_, request)) = &self.merged_cleanup.removing else {
            return;
        };
        let request = request.clone();
        let Some(result) = self.take_answer(crate::state::Answer::Cleanup, &request) else {
            return;
        };
        let Some((candidate, _)) = self.merged_cleanup.removing.take() else {
            return;
        };
        let refusal = match result {
            Err(error) => Some(error.to_string()),
            Ok(response) => response
                .get("error")
                .filter(|error| !error.is_null())
                .map(|error| {
                    let (code, message) = menu::endpoint_error(error);
                    format!("{code}: {message}")
                }),
        };
        match refusal {
            None => {
                let number = candidate.number;
                self.merged_cleanup.ledger.settle(candidate, None);
                self.show_flash(
                    crate::window::Flash::success(format!(
                        "Removed the worktree of merged pull request #{number}"
                    )),
                    cx,
                );
            }
            Some(error) => {
                tracing::warn!(category = "refused", "Merged worktree kept");
                self.merged_cleanup
                    .ledger
                    .settle(candidate, Some(Instant::now() + RETRY));
                self.local_error = Some(format!("Remove merged worktree: {error}"));
                cx.notify();
            }
        }
    }
}

#[cfg(test)]
mod tests;
