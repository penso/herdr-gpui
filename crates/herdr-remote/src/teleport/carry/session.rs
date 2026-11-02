//! Session-only carries never resolve repositories or run Git. Directory
//! probes, transcript transfer and daemon commands all run on the carry worker.

use super::*;
use crate::teleport::{
    Host,
    snapshot::{ProcessInfoResult, WorkspaceCreated},
};
use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};

/// How long the resumed agent may take to become the destination pane's
/// foreground program. Short in tests, which simulate a start that never comes.
const LAUNCH_WAIT: Duration = if cfg!(test) {
    Duration::from_millis(600)
} else {
    Duration::from_secs(20)
};
const LAUNCH_POLL: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionSource {
    cwd: String,
    relative: String,
    work: Work,
}

#[derive(Debug, Clone)]
pub struct SessionReview {
    destination: Place,
    pub cwd: String,
    source: SessionSource,
}

impl SessionReview {
    /// The pane's work and the resume the destination runs for it, for the
    /// review to list; `None` for a pane with no agent session.
    pub fn resume(&self) -> Option<(&Work, Action)> {
        let Work::Session { agent, .. } = &self.source.work else {
            return None;
        };
        Some((&self.source.work, Action::Resume(*agent)))
    }
}

/// A review of `agent`'s session moving into `cwd` on `destination`, for
/// window tests that need a session-only move ready to start.
#[cfg(any(test, feature = "test-support"))]
pub fn session_review(destination: Place, cwd: &str, agent: AgentKind) -> Review {
    let session = crate::teleport::snapshot::AgentSession {
        agent: agent.binary().into(),
        kind: crate::teleport::snapshot::SessionKind::Id,
        source: "test".into(),
        value: "session".into(),
    };
    Review::SessionOnly(Box::new(SessionReview {
        destination,
        cwd: cwd.into(),
        source: SessionSource {
            cwd: cwd.into(),
            relative: cwd.into(),
            work: Work::Session {
                agent,
                session,
                flags: Vec::new(),
            },
        },
    }))
}

pub fn check_session(origin: &Origin, cancelled: &AtomicBool) -> Result<()> {
    session_source(origin, cancelled).map(drop)
}

/// Read the authoritative session and cwd. The GUI snapshot has neither
/// session references nor launch argv, so picker availability is checked here.
fn session_source(origin: &Origin, cancelled: &AtomicBool) -> Result<SessionSource> {
    let from = &origin.place.host;
    let snapshot: SnapshotResult = from.herdr(Step::Review, &["api", "snapshot"], cancelled)?;
    let workspace = snapshot
        .snapshot
        .workspace(&origin.workspace_id)
        .ok_or(Error::WorkspaceGone)?;
    if workspace.worktree.is_some() {
        return Err(Error::SourceSessionChanged);
    }
    let pane = snapshot
        .snapshot
        .pane(&origin.pane_id)
        .ok_or(Error::PaneGone)?;
    // Read from the host, not the window's snapshot, which can lag: a working
    // agent is still writing the session it would leave behind.
    if pane.working {
        return Err(Error::SessionAgentWorking);
    }
    if !snapshot
        .snapshot
        .tabs_of(&origin.workspace_id)
        .iter()
        .any(|tab| tab.tab_id == pane.tab_id)
    {
        return Err(Error::PaneGone);
    }
    let process = plan::process_infos(from, std::slice::from_ref(&origin.pane_id), cancelled)?
        .remove(&origin.pane_id)
        .unwrap_or_default();
    let work = Work::classify(pane, &process);
    if !matches!(work, Work::Session { .. }) {
        return Err(Error::SessionRequired);
    }
    let cwd = process
        .foreground_job()
        .and_then(|job| job.cwd.as_deref())
        .or(pane.foreground_cwd.as_deref())
        .or(pane.cwd.as_deref())
        .ok_or(Error::SessionCwdUnavailable)?;
    // Compare physical paths: a home reached through a symlink (`/home` ->
    // `/var/home`) must still count as home.
    let home = physical(from, None, cancelled)?;
    let relative = relative_cwd(&home, &physical(from, Some(cwd), cancelled)?)?;
    Ok(SessionSource {
        cwd: cwd.to_owned(),
        relative,
        work,
    })
}

/// Require an absolute, home-relative path with no traversal components.
fn relative_cwd(home: &str, cwd: &str) -> Result<String> {
    let home = home.trim_end_matches('/');
    let valid = |path: &str| {
        path.starts_with('/')
            && !path.chars().any(char::is_control)
            && !path.split('/').any(|part| matches!(part, "." | ".."))
    };
    if home.is_empty() || !valid(home) || !valid(cwd) {
        return Err(Error::OutsideHome {
            path: cwd.to_owned(),
        });
    }
    remap(cwd, home, "")
        .map(|rest| rest.trim_start_matches('/').to_owned())
        .ok_or_else(|| Error::OutsideHome {
            path: cwd.to_owned(),
        })
}

/// `dir`, or the home directory when `None`, with symlinks resolved on `host`.
fn physical(host: &Host, dir: Option<&str>, cancelled: &AtomicBool) -> Result<String> {
    let dir = dir.map_or_else(|| "\"$HOME\"".to_owned(), shell_quote);
    let output = host.query(
        Step::Review,
        &format!("cd -P -- {dir} && pwd -P\n"),
        &[],
        cancelled,
    )?;
    let path = std::str::from_utf8(&output)
        .map_err(|_| Error::SessionCwdUnavailable)?
        .trim_end_matches('\n');
    if !path.starts_with('/') || path.chars().any(char::is_control) {
        return Err(Error::SessionCwdUnavailable);
    }
    Ok(path.to_owned())
}

fn require_directory(host: &Host, cwd: &str, cancelled: &AtomicBool) -> Result<()> {
    let output = host.query(
        Step::Review,
        &format!("if [ -d {} ]; then printf yes; fi\n", shell_quote(cwd)),
        &[],
        cancelled,
    )?;
    if output != b"yes" {
        return Err(Error::DestinationDirectoryMissing {
            path: cwd.to_owned(),
        });
    }
    Ok(())
}

pub(super) fn review(
    origin: &Origin,
    destination: &Place,
    cancelled: &AtomicBool,
) -> Result<SessionReview> {
    let source = session_source(origin, cancelled)?;
    let home = physical(&destination.host, None, cancelled)?;
    let cwd = if source.relative.is_empty() {
        home
    } else {
        format!("{home}/{}", source.relative)
    };
    require_directory(&destination.host, &cwd, cancelled)?;
    let Work::Session { agent, .. } = &source.work else {
        return Err(Error::SessionRequired);
    };
    if destination
        .host
        .installed(Step::Review, &[agent.binary()], cancelled)?
        .is_empty()
    {
        return Err(Error::SessionProgramMissing {
            agent: agent.binary(),
            host: destination.label.clone(),
        });
    }
    Ok(SessionReview {
        destination: destination.clone(),
        cwd,
        source,
    })
}

pub(super) fn run(
    origin: &Origin,
    review: &SessionReview,
    report: &mut impl FnMut(Step),
    cancelled: &AtomicBool,
) -> Result<Carried> {
    // Confirm the session identity, flags and cwd before copying anything or
    // closing the source. A session-only move must never start afresh.
    if session_source(origin, cancelled)? != review.source {
        return Err(Error::SourceSessionChanged);
    }
    let to = &review.destination.host;
    require_directory(to, &review.cwd, cancelled)?;
    let Work::Session {
        agent,
        session,
        flags,
    } = &review.source.work
    else {
        return Err(Error::SessionRequired);
    };
    // Open the workspace first, so the failures most likely to happen there
    // come before anything is written into the destination's agent storage.
    report(Step::Open);
    let made: WorkspaceCreated = to.herdr(
        Step::Open,
        &["workspace", "create", "--cwd", &review.cwd, "--no-focus"],
        cancelled,
    )?;
    let workspace_id = made.workspace.workspace_id;
    let launched = (|| {
        let snapshot: SnapshotResult = to.herdr(Step::Open, &["api", "snapshot"], cancelled)?;
        let tab = snapshot
            .snapshot
            .tabs_of(&workspace_id)
            .first()
            .copied()
            .ok_or(Error::PaneGone)?;
        let pane = snapshot
            .snapshot
            .panes
            .iter()
            .find(|pane| pane.tab_id == tab.tab_id)
            .ok_or(Error::PaneGone)?;
        report(Step::Sessions);
        let argv = resume_session(
            *agent,
            session,
            flags,
            &Route {
                source: &origin.place.host,
                destination: to,
                from: &review.source.cwd,
                to: &review.cwd,
                cwd: &review.cwd,
            },
            cancelled,
        )?;
        report(Step::Launch);
        // Explicit cd also refuses if the directory disappears after review.
        let line = format!(
            "cd -- {} && {}",
            shell_quote(&review.cwd),
            command_line(&argv)
        );
        to.herdr_ok(
            Step::Launch,
            &["pane", "run", &pane.pane_id, &line],
            cancelled,
        )?;
        // `pane run` only queues the line; the source pane must not close
        // until the agent really runs there.
        wait_for_agent(
            to,
            &pane.pane_id,
            *agent,
            &review.destination.label,
            cancelled,
        )?;
        Ok(pane.pane_id.clone())
    })();
    let pane_id = match launched {
        Ok(pane) => pane,
        Err(cause) => {
            return Err(
                match to.herdr_ok(
                    Step::Open,
                    &["workspace", "close", &workspace_id],
                    &AtomicBool::new(false),
                ) {
                    Ok(()) => cause,
                    Err(_) => Error::SessionWorkspaceLeftBehind {
                        workspace_id,
                        cause: Box::new(cause),
                    },
                },
            );
        }
    };
    let mut warnings = Vec::new();
    retire(origin, &mut warnings, report);
    Ok(Carried {
        endpoint_id: review.destination.endpoint_id.clone(),
        workspace_id,
        pane_id,
        warnings,
    })
}

/// Wait until `agent` is the foreground program of `pane` on `host`. Agents
/// often run through an interpreter (`node .../claude`, `sh .../claude`), so
/// the program name may be the first or second argument.
fn wait_for_agent(
    host: &Host,
    pane: &str,
    agent: AgentKind,
    label: &str,
    cancelled: &AtomicBool,
) -> Result<()> {
    let deadline = Instant::now() + LAUNCH_WAIT;
    loop {
        let info: ProcessInfoResult = host.herdr(
            Step::Launch,
            &["pane", "process-info", "--pane", pane],
            cancelled,
        )?;
        let started = info.process_info.foreground_job().is_some_and(|job| {
            job.argv
                .iter()
                .take(2)
                .any(|arg| arg.rsplit('/').next() == Some(agent.binary()))
        });
        if started {
            return Ok(());
        }
        if cancelled.load(Ordering::Acquire) {
            return Err(Error::Cancelled);
        }
        if Instant::now() >= deadline {
            return Err(Error::SessionNotStarted {
                agent: agent.binary(),
                host: label.to_owned(),
            });
        }
        std::thread::sleep(LAUNCH_POLL);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[path = "tests/session_mapping.rs"]
mod tests;

#[cfg(any(test, feature = "test-support"))]
impl SessionReview {
    /// A ready session-only review for window tests; it never runs scripts.
    pub fn fixture(destination: Place) -> Self {
        Self {
            destination,
            cwd: "/home/there/notes".into(),
            source: SessionSource {
                cwd: "/home/here/notes".into(),
                relative: "notes".into(),
                work: Work::Session {
                    agent: AgentKind::Claude,
                    session: crate::teleport::snapshot::AgentSession {
                        agent: "claude".into(),
                        source: "herdr:claude".into(),
                        kind: crate::teleport::snapshot::SessionKind::Id,
                        value: "session-603".into(),
                    },
                    flags: Vec::new(),
                },
            },
        }
    }
}
