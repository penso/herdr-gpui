//! A fresh, cancellable preflight owned by one close dialog. Unknown status
//! requires the same explicit consent as work that has not been published.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

use gpui::App;
use herdr_client::protocol::{ClientShellSnapshot, ClientShellWorktree};

use super::workspace::WorkspaceTarget;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Checkout {
    id: String,
    worktree: Option<ClientShellWorktree>,
    branch: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_and_risky_reports_cannot_close_without_consent() {
        let snapshot = crate::sidebar::layout_tests::snapshot(7);
        let target = WorkspaceTarget::new(&snapshot, &snapshot.workspaces[3]);
        let mut check = CloseCheck::fixture(&snapshot, &target, None);
        assert!(!check.ready("close"));
        for report in [
            Report {
                dirty: true,
                ..Report::default()
            },
            Report {
                unpushed: true,
                ..Report::default()
            },
            Report {
                unknown: true,
                ..Report::default()
            },
        ] {
            check.report = Some(report);
            for text in ["", "Close", "yes", "clos"] {
                assert!(!check.ready(text));
            }
            assert!(check.ready("close"));
        }
        check.report = Some(Report::default());
        assert!(check.ready(""));
    }

    #[test]
    fn changed_sibling_checkout_invalidates_group_consent() {
        let mut snapshot = crate::sidebar::layout_tests::snapshot(7);
        let target = WorkspaceTarget::new(&snapshot, &snapshot.workspaces[3]);
        let check = CloseCheck::fixture(&snapshot, &target, Some(Report::default()));
        assert!(check.current(&snapshot, &target));
        snapshot.workspaces[4].branch = Some("replacement".into());
        assert!(!check.current(&snapshot, &target));
    }

    #[test]
    fn disconnected_worker_requires_consent_and_drop_cancels() {
        let snapshot = crate::sidebar::layout_tests::snapshot(7);
        let target = WorkspaceTarget::new(&snapshot, &snapshot.workspaces[3]);
        let mut check = CloseCheck::fixture(&snapshot, &target, None);
        assert!(check.poll());
        assert!(!check.ready(""));
        assert!(check.ready("close"));
        let cancelled = check.cancelled.clone();
        drop(check);
        assert!(cancelled.load(Ordering::Relaxed));
    }
}

pub(super) const CHECKING: &str = "Checking for uncommitted files and unpushed commits...";

#[derive(Default)]
pub(super) struct Report {
    pub dirty: bool,
    pub unpushed: bool,
    pub unknown: bool,
}

impl Report {
    pub fn needs_consent(&self) -> bool {
        self.dirty || self.unpushed || self.unknown
    }
}

pub(super) struct CloseCheck {
    checkouts: Vec<Checkout>,
    receiver: mpsc::Receiver<Report>,
    cancelled: Arc<AtomicBool>,
    pub report: Option<Report>,
}

impl Drop for CloseCheck {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

fn checkouts(snapshot: &ClientShellSnapshot, target: &WorkspaceTarget) -> Vec<Checkout> {
    snapshot
        .workspaces
        .iter()
        .filter(|workspace| target.close_members.contains(&workspace.workspace_id))
        .map(|workspace| Checkout {
            id: workspace.workspace_id.clone(),
            worktree: workspace.worktree.clone(),
            branch: workspace.branch.clone(),
        })
        .collect()
}

impl CloseCheck {
    #[cfg(test)]
    pub(super) fn fixture(
        snapshot: &ClientShellSnapshot,
        target: &WorkspaceTarget,
        report: Option<Report>,
    ) -> Self {
        let (_, receiver) = mpsc::sync_channel(1);
        Self {
            checkouts: checkouts(snapshot, target),
            receiver,
            cancelled: Arc::new(AtomicBool::new(false)),
            report,
        }
    }

    pub fn start(
        snapshot: &ClientShellSnapshot,
        target: &WorkspaceTarget,
        local: bool,
        cx: &App,
    ) -> Self {
        let checkouts = checkouts(snapshot, target);
        let inputs: Vec<_> = checkouts
            .iter()
            .take(128)
            .map(|checkout| {
                crate::pull_request::repository_input(
                    checkout.worktree.as_ref(),
                    checkout.branch.as_deref(),
                )
            })
            .collect();
        let mut report = Report {
            unknown: checkouts.len() > 128,
            ..Report::default()
        };
        let (sender, receiver) = mpsc::sync_channel(1);
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancellation = cancelled.clone();
        cx.background_executor()
            .spawn(async move {
                let deadline = Instant::now() + Duration::from_secs(15);
                for input in inputs {
                    if cancellation.load(Ordering::Relaxed) {
                        return;
                    }
                    let status = input.and_then(|input| {
                        if !local {
                            return Err(crate::pull_request::Error::UntrustedEndpoint.into());
                        }
                        crate::git::close_status(&input, deadline, &|| {
                            cancellation.load(Ordering::Relaxed)
                        })
                    });
                    match status {
                        Ok((dirty, unpushed)) => {
                            report.dirty |= dirty;
                            report.unpushed |= unpushed;
                        }
                        Err(_) => report.unknown = true,
                    }
                }
                let _ = sender.send(report);
            })
            .detach();
        Self {
            checkouts,
            receiver,
            cancelled,
            report: None,
        }
    }

    pub fn poll(&mut self) -> bool {
        if self.report.is_some() {
            return false;
        }
        match self.receiver.try_recv() {
            Ok(report) => self.report = Some(report),
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.report = Some(Report {
                    unknown: true,
                    ..Report::default()
                })
            }
        }
        true
    }

    pub fn current(&self, snapshot: &ClientShellSnapshot, target: &WorkspaceTarget) -> bool {
        self.checkouts == checkouts(snapshot, target)
    }

    /// Whether the close may go without the dialog once
    /// `confirm_close_workspace` is off: the check finished without finding
    /// uncommitted files or unpushed commits. Unverifiable status does not
    /// hold it, since a workspace outside a Herdr worktree is never
    /// verifiable and turning confirmation off asked not to be stopped.
    pub fn skippable(&self, confirm: bool) -> bool {
        !confirm
            && self
                .report
                .as_ref()
                .is_some_and(|report| !report.dirty && !report.unpushed)
    }

    /// Whether the close may be sent, with `text` typed as consent.
    pub fn allows(&self, text: &str, confirm: bool) -> bool {
        self.ready(text) || self.skippable(confirm)
    }

    /// The dialog's Git line. It asks for typed consent only while the
    /// close needs it, since the input shows only then.
    pub fn status(&self, confirm: bool) -> String {
        let Some(report) = &self.report else {
            return CHECKING.to_owned();
        };
        let mut warnings = Vec::new();
        if report.dirty {
            warnings.push("Uncommitted files are present.");
        }
        if report.unpushed {
            warnings.push("Unpushed commits are present.");
        }
        if report.unknown {
            warnings.push("Git status could not be verified for every checkout.");
        }
        if !report.needs_consent() {
            warnings.push(
                "No uncommitted files or unpushed commits found (using local remote-tracking refs).",
            );
        } else if !self.skippable(confirm) {
            warnings.push("Type close to consent to closing anyway, or Cancel to keep working.");
        }
        warnings.join(" ")
    }

    pub fn ready(&self, text: &str) -> bool {
        self.report
            .as_ref()
            .is_some_and(|report| !report.needs_consent() || text == "close")
    }
}
