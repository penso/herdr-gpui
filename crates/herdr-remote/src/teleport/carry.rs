//! Carry one pane to another host. The checkout's commits and uncommitted
//! changes are copied into a new worktree of the same repository there, or
//! into an existing space of it that is clean and can fast-forward to them;
//! the pane's agent session moves with them and resumes, and the source pane
//! closes. Unlike a teleport, the source checkout and the rest of its
//! workspace stay as they are. Outside a repository, only the agent session
//! moves, into an existing home-relative directory on the destination.

use super::{
    error::{Error, Step},
    git,
    job::{self, Destination, HostRepositories, Place, Repository, Source, plan},
    launch::{
        AgentKind, Work, command_line, handoff_path, handoff_prompt, handoff_resume_prompt, remap,
        remap_argv,
    },
    remote::MatchReason,
    sessions::{Route, move_session},
    snapshot::{SnapshotResult, TabCreated, WorktreeCreated},
    warning::Warning,
};
use herdr_client::shell_quote;
use std::{collections::HashMap, io::Seek, sync::atomic::AtomicBool};

pub use {super::error::Result, plan::Action};

/// The pane being carried, as the GUI snapshot describes it.
#[derive(Debug, Clone)]
pub struct Origin {
    pub place: Place,
    pub workspace_id: String,
    pub pane_id: String,
}

mod session;
#[cfg(any(test, feature = "test-support"))]
pub use session::session_review;
pub use session::{SessionReview, check_session};

/// Where on the destination the user chose to put the pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Choice {
    // keep-sorted start
    NewWorktree,
    SessionOnly,
    /// An open workspace of the repository.
    Space {
        workspace_id: String,
    },
    // keep-sorted end
}

/// How the work lands, as the review found it can.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Landing {
    /// A new worktree: on the source branch when the destination can take
    /// it as it is, otherwise on a branch of its own from the same commit.
    Worktree { branch: Option<String> },
    /// An existing space's clean checkout, at the source's commit or behind
    /// it on the same branch, when it fast-forwards first. The pane gets a
    /// new tab there.
    Space {
        workspace_id: String,
        checkout: String,
        /// The checkout's commit at the review; it must not move meanwhile.
        head: String,
        fast_forward: bool,
    },
}

/// What the move will do, for the user to confirm.
#[derive(Debug, Clone)]
pub struct RepositoryReview {
    pub destination: Place,
    /// The destination's matching repository, which the worktree joins.
    pub repository: Repository,
    pub checkout: String,
    pub state: git::SourceState,
    pub landing: Landing,
    /// The directory the pane's program runs in.
    pub cwd: Option<String>,
    pub work: Work,
    pub action: Action,
}

#[derive(Debug, Clone)]
pub enum Review {
    // keep-sorted start
    Repository(Box<RepositoryReview>),
    SessionOnly(Box<SessionReview>),
    // keep-sorted end
}

/// Where the pane went.
#[derive(Debug)]
pub struct Carried {
    pub endpoint_id: String,
    pub workspace_id: String,
    pub pane_id: String,
    /// Things that did not carry over, for the user to see.
    pub warnings: Vec<Warning>,
}

/// Read the pane and the destination, and decide what the move does.
pub fn review(
    origin: &Origin,
    destination: &HostRepositories,
    choice: &Choice,
    cancelled: &AtomicBool,
) -> Result<Review> {
    match choice {
        Choice::SessionOnly => session::review(origin, &destination.place, cancelled)
            .map(|review| Review::SessionOnly(Box::new(review))),
        Choice::NewWorktree | Choice::Space { .. } => {
            review_repository(origin, destination, choice, cancelled)
                .map(|review| Review::Repository(Box::new(review)))
        }
    }
}

fn review_repository(
    origin: &Origin,
    destination: &HostRepositories,
    choice: &Choice,
    cancelled: &AtomicBool,
) -> Result<RepositoryReview> {
    let from = &origin.place.host;
    let to = &destination.place.host;
    let snapshot: SnapshotResult = from.herdr(Step::Review, &["api", "snapshot"], cancelled)?;
    let snapshot = snapshot.snapshot;
    let pane = snapshot.pane(&origin.pane_id).ok_or(Error::PaneGone)?;
    let worktree = snapshot
        .workspace(&origin.workspace_id)
        .and_then(|workspace| workspace.worktree.clone())
        .ok_or(Error::WorkspaceGone)?;
    // A space is checked against its own repository, which need not be the
    // one a device with several clones would otherwise be matched by.
    let space = match choice {
        Choice::NewWorktree => None,
        Choice::SessionOnly => return Err(Error::SessionRequired),
        Choice::Space { workspace_id } => Some(open_space(destination, workspace_id, cancelled)?),
    };
    let narrowed;
    let hosts = match &space {
        Some(space) => {
            narrowed = HostRepositories {
                place: destination.place.clone(),
                repositories: Some(vec![Repository {
                    key: space.repo_key.clone(),
                    label: space.repo_name.clone(),
                    workspace_id: space.workspace_id.clone(),
                }]),
                retired: Vec::new(),
            };
            &narrowed
        }
        None => destination,
    };
    let source = Source {
        place: origin.place.clone(),
        workspace_id: origin.workspace_id.clone(),
        custom_label: None,
        repo_key: worktree.repo_key.clone(),
        repo_label: worktree.repo_name.clone(),
        branch: None,
        tab_labels: HashMap::new(),
    };
    let candidate = job::resolve(&source, hosts, cancelled)?;
    // Only a repository sharing a remote is the same project; a name alone
    // could be anything.
    let repository = match candidate.destination {
        Destination::Open {
            repository,
            reason: MatchReason::Origin | MatchReason::Remote,
        } => repository,
        _ => {
            return Err(Error::RepositoryNotOpen {
                host: destination.place.label.clone(),
            });
        }
    };
    let checkout = worktree.checkout_path;
    let state = git::source_state(from, &checkout, cancelled)?;
    let landing = match space {
        None => Landing::Worktree {
            branch: worktree_branch(
                origin,
                destination,
                &repository,
                &checkout,
                &state,
                cancelled,
            )?,
        },
        Some(space) => space_landing(origin, destination, &checkout, &state, space, cancelled)?,
    };
    let process = plan::process_infos(from, std::slice::from_ref(&origin.pane_id), cancelled)?
        .remove(&origin.pane_id)
        .unwrap_or_default();
    let cwd = process
        .foreground_job()
        .and_then(|job| job.cwd.clone())
        .or_else(|| pane.foreground_cwd.clone())
        .or_else(|| pane.cwd.clone());
    if let Some(cwd) = &cwd
        && remap(cwd, &checkout, &checkout).is_none()
    {
        return Err(Error::OutsideCheckout { path: cwd.clone() });
    }
    let work = Work::classify(pane, &process);
    let mut programs: Vec<&str> = AgentKind::FALLBACKS.iter().map(|k| k.binary()).collect();
    programs.extend(work.program());
    programs.sort_unstable();
    programs.dedup();
    let installed = to.installed(Step::Review, &programs, cancelled)?;
    let action = plan::plan_action(&work, &installed, || handoff_path(0));
    Ok(RepositoryReview {
        destination: destination.place.clone(),
        repository,
        checkout,
        state,
        landing,
        cwd,
        work,
        action,
    })
}

/// The source branch, when the destination can take it as it is.
fn worktree_branch(
    origin: &Origin,
    destination: &HostRepositories,
    repository: &Repository,
    checkout: &str,
    state: &git::SourceState,
    cancelled: &AtomicBool,
) -> Result<Option<String>> {
    let Some(branch) = &state.branch else {
        return Ok(None);
    };
    let to = &destination.place.host;
    let dest = git::destination_branch(to, &repository.key, branch, cancelled)?;
    match git::check_destination(&origin.place.host, checkout, branch, &dest, cancelled) {
        Ok(()) => Ok(Some(branch.clone())),
        Err(Error::BranchCheckedOut { .. } | Error::BranchDiverged { .. }) => Ok(None),
        Err(error) => Err(error),
    }
}

/// The space `workspace_id` on the destination, with its checkout.
struct OpenSpace {
    workspace_id: String,
    repo_key: String,
    repo_name: String,
    checkout_path: String,
}

fn open_space(
    destination: &HostRepositories,
    workspace_id: &str,
    cancelled: &AtomicBool,
) -> Result<OpenSpace> {
    let to = &destination.place.host;
    let snapshot: SnapshotResult = to.herdr(Step::Review, &["api", "snapshot"], cancelled)?;
    let tree = snapshot
        .snapshot
        .workspace(workspace_id)
        .and_then(|workspace| workspace.worktree.clone())
        .ok_or_else(|| Error::SpaceGone {
            host: destination.place.label.clone(),
        })?;
    Ok(OpenSpace {
        workspace_id: workspace_id.to_owned(),
        repo_key: tree.repo_key,
        repo_name: tree.repo_name,
        checkout_path: tree.checkout_path,
    })
}

/// Whether `space` can take the work: a checkout with nothing uncommitted
/// to overwrite, at the source's commit or behind it on the same branch.
fn space_landing(
    origin: &Origin,
    destination: &HostRepositories,
    checkout: &str,
    state: &git::SourceState,
    space: OpenSpace,
    cancelled: &AtomicBool,
) -> Result<Landing> {
    let to = &destination.place.host;
    let there = git::source_state(to, &space.checkout_path, cancelled)?;
    if there.changed > 0 {
        return Err(Error::SpaceChanged {
            path: space.checkout_path,
        });
    }
    let fast_forward = if there.head == state.head {
        false
    } else if there.branch.is_some()
        && there.branch == state.branch
        && git::source_contains(&origin.place.host, checkout, &there.head, cancelled)?
    {
        true
    } else {
        return Err(Error::SpaceDiverged {
            branch: there.branch.unwrap_or_else(|| "a detached HEAD".to_owned()),
        });
    };
    Ok(Landing::Space {
        workspace_id: space.workspace_id,
        checkout: space.checkout_path,
        head: there.head,
        fast_forward,
    })
}

/// Copy the work and move the pane. `report` announces each step as it
/// starts. Nothing on the source changes until the pane runs on the
/// destination, and then only the source pane closes.
pub fn run(
    origin: &Origin,
    review: &Review,
    mut report: impl FnMut(Step),
    cancelled: &AtomicBool,
) -> Result<Carried> {
    match review {
        Review::Repository(review) => run_repository(origin, review, report, cancelled),
        Review::SessionOnly(review) => session::run(origin, review, &mut report, cancelled),
    }
}

fn run_repository(
    origin: &Origin,
    review: &RepositoryReview,
    mut report: impl FnMut(Step),
    cancelled: &AtomicBool,
) -> Result<Carried> {
    let from = &origin.place.host;
    let to = &review.destination.host;
    let key = &review.repository.key;
    let mut warnings = Vec::new();

    // The note is written into the source checkout first, so it travels with
    // the uncommitted changes.
    if let Action::Handoff { note, .. } = &review.action {
        report(Step::Handoff);
        let timeout = job::HANDOFF_TIMEOUT.as_millis().to_string();
        let prompt = handoff_prompt(&format!("{}/{note}", review.checkout));
        let asked = from.herdr_ok(
            Step::Handoff,
            &[
                "agent",
                "prompt",
                &origin.pane_id,
                &prompt,
                "--wait",
                "--timeout",
                &timeout,
            ],
            cancelled,
        );
        match asked {
            Err(Error::Cancelled) => return Err(Error::Cancelled),
            Err(error) => warnings.push(Warning::NoHandoffNote {
                pane: None,
                source: error,
            }),
            Ok(()) => {}
        }
    }

    report(Step::Capture);
    let tips = git::destination_branch(
        to,
        key,
        review.state.branch.as_deref().unwrap_or("HEAD"),
        cancelled,
    )?
    .tips;
    let reference = job::reference_name();
    let mut bundle = tempfile::tempfile().map_err(Error::LocalFile)?;
    git::capture(
        from,
        &review.checkout,
        &reference,
        &tips,
        &mut bundle,
        cancelled,
    )?;
    bundle.rewind().map_err(Error::LocalFile)?;
    report(Step::Transfer);
    let uploaded = git::upload(to, bundle, cancelled)?;
    report(Step::Fetch);
    let fetched = git::fetch(to, key, &uploaded, &reference, cancelled);
    let _ = git::discard_upload(to, &uploaded, cancelled);
    fetched?;
    // The review promised this commit; one made since would arrive as
    // uncommitted changes in a space at the reviewed commit.
    let shipped = git::resolve_commit(to, key, &format!("{reference}~2"), cancelled);
    if let Err(error) = shipped.and_then(|commit| {
        (commit == review.state.head)
            .then_some(())
            .ok_or(Error::SourceMoved)
    }) {
        git::drop_reference(to, key, &reference, &AtomicBool::new(false));
        return Err(error);
    }

    let landed = match &review.landing {
        Landing::Worktree { branch } => into_worktree(
            review,
            branch.as_deref(),
            from,
            &reference,
            &mut warnings,
            &mut report,
            cancelled,
        ),
        Landing::Space {
            workspace_id,
            checkout,
            head,
            fast_forward,
        } => into_space(
            review,
            from,
            &Space {
                workspace_id,
                checkout,
                head,
                fast_forward: *fast_forward,
            },
            &reference,
            &mut warnings,
            &mut report,
            cancelled,
        ),
    };
    let (workspace_id, pane_id) = match landed {
        Ok(landed) => landed,
        Err(error) => {
            git::drop_reference(to, key, &reference, &AtomicBool::new(false));
            return Err(error);
        }
    };

    // The work runs on the destination now; a source pane that will not
    // close is only worth a warning.
    retire(origin, &mut warnings, &mut report);

    Ok(Carried {
        endpoint_id: review.destination.endpoint_id.clone(),
        workspace_id,
        pane_id,
        warnings,
    })
}

fn retire(origin: &Origin, warnings: &mut Vec<Warning>, report: &mut impl FnMut(Step)) {
    report(Step::Retire);
    if let Err(error) = origin.place.host.herdr_ok(
        Step::Retire,
        &["pane", "close", &origin.pane_id],
        &AtomicBool::new(false),
    ) {
        warnings.push(Warning::SourcePaneOpen { source: error });
    }
}

/// Land the work in a new worktree and start the pane's program in its first
/// pane. Until the program starts, the worktree holds nothing but the copied
/// work, so a failure removes it again. Returns its workspace and pane.
fn into_worktree(
    review: &RepositoryReview,
    branch: Option<&str>,
    from: &super::Host,
    reference: &str,
    warnings: &mut Vec<Warning>,
    report: &mut impl FnMut(Step),
    cancelled: &AtomicBool,
) -> Result<(String, String)> {
    let to = &review.destination.host;
    let created = create_worktree(review, branch, reference, report, cancelled)?;
    let path = &created.worktree.path;
    let pane = &created.root_pane.pane_id;
    let landed = (|| {
        report(Step::Restore);
        git::restore(to, path, reference, cancelled)?;
        launch(review, from, path, pane, warnings, report, cancelled)
    })();
    if let Err(error) = landed {
        let workspace = &created.workspace.workspace_id;
        let removed = to.herdr_ok(
            Step::CreateWorktree,
            &["worktree", "remove", "--workspace", workspace, "--force"],
            &AtomicBool::new(false),
        );
        return Err(match removed {
            Ok(()) => error,
            Err(_) => Error::LeftBehind {
                path: path.clone(),
                cause: Box::new(error),
            },
        });
    }
    Ok((created.workspace.workspace_id, created.root_pane.pane_id))
}

/// An existing space the work lands in; see [`Landing::Space`].
struct Space<'a> {
    workspace_id: &'a str,
    checkout: &'a str,
    head: &'a str,
    fast_forward: bool,
}

/// Land the work in an existing space's checkout and start the pane's
/// program in a new tab there. The copied changes stay if the program cannot
/// start; the tab made for it closes. Returns the workspace and pane.
#[allow(clippy::too_many_arguments)]
fn into_space(
    review: &RepositoryReview,
    from: &super::Host,
    space: &Space<'_>,
    reference: &str,
    warnings: &mut Vec<Warning>,
    report: &mut impl FnMut(Step),
    cancelled: &AtomicBool,
) -> Result<(String, String)> {
    let to = &review.destination.host;
    report(Step::Restore);
    git::apply(
        to,
        space.checkout,
        space.head,
        space.fast_forward,
        reference,
        cancelled,
    )?;
    report(Step::Tabs);
    let made: TabCreated = to.herdr(
        Step::Tabs,
        &[
            "tab",
            "create",
            "--workspace",
            space.workspace_id,
            "--cwd",
            space.checkout,
            "--no-focus",
        ],
        cancelled,
    )?;
    let pane = &made.root_pane.pane_id;
    if let Err(error) = launch(
        review,
        from,
        space.checkout,
        pane,
        warnings,
        report,
        cancelled,
    ) {
        let _ = to.herdr_ok(
            Step::Tabs,
            &["tab", "close", &made.tab.tab_id],
            &AtomicBool::new(false),
        );
        return Err(error);
    }
    Ok((space.workspace_id.to_owned(), made.root_pane.pane_id))
}

/// Move the agent's session when there is one, and run the pane's program in
/// `pane`, a fresh shell in `path`, from the pane's own subdirectory.
fn launch(
    review: &RepositoryReview,
    from: &super::Host,
    path: &str,
    pane: &str,
    warnings: &mut Vec<Warning>,
    report: &mut impl FnMut(Step),
    cancelled: &AtomicBool,
) -> Result<()> {
    report(Step::Sessions);
    let argv = destination_argv(review, from, path, warnings, cancelled)?;
    let cwd = review
        .cwd
        .as_deref()
        .and_then(|cwd| remap(cwd, &review.checkout, path))
        .unwrap_or_else(|| path.to_owned());
    // A shell, or a program missing there, still lands in its subdirectory.
    let cd = (cwd != path).then(|| format!("cd -- {}", shell_quote(&cwd)));
    let line = match (cd, argv) {
        (Some(cd), Some(argv)) => format!("{cd} && {}", command_line(&argv)),
        (None, Some(argv)) => command_line(&argv),
        (Some(cd), None) => cd,
        (None, None) => return Ok(()),
    };
    report(Step::Launch);
    review
        .destination
        .host
        .herdr_ok(Step::Launch, &["pane", "run", pane, &line], cancelled)
}

/// A worktree on the destination at the source's HEAD: on the source's
/// branch when the destination can take it, else on a branch the daemon
/// names.
fn create_worktree(
    review: &RepositoryReview,
    branch: Option<&str>,
    reference: &str,
    report: &mut impl FnMut(Step),
    cancelled: &AtomicBool,
) -> Result<WorktreeCreated> {
    let to = &review.destination.host;
    let key = &review.repository.key;
    let base;
    let mut args = vec![
        "worktree",
        "create",
        "--workspace",
        &review.repository.workspace_id,
        "--no-focus",
    ];
    // Another checkout there may have taken the branch since the review;
    // moving its ref would leave that checkout's files behind.
    let free = match branch {
        Some(branch) => git::destination_branch(to, key, branch, cancelled)?
            .checked_out
            .is_none()
            .then_some(branch),
        None => None,
    };
    if let Some(branch) = free {
        git::advance_branch(to, key, branch, reference, cancelled)?;
        args.extend(["--branch", branch]);
    } else {
        base = git::resolve_commit(to, key, &format!("{reference}~2"), cancelled)?;
        args.extend(["--base", &base]);
    }
    report(Step::CreateWorktree);
    to.herdr(Step::CreateWorktree, &args, cancelled)
}

/// What the destination pane runs, after moving the agent's session when it
/// has one.
fn destination_argv(
    review: &RepositoryReview,
    from: &super::Host,
    path: &str,
    warnings: &mut Vec<Warning>,
    cancelled: &AtomicBool,
) -> Result<Option<Vec<String>>> {
    Ok(match &review.action {
        Action::Shell => None,
        Action::Missing(program) => {
            warnings.push(Warning::ProgramMissing {
                program: program.clone(),
                host: review.destination.label.clone(),
            });
            None
        }
        Action::Resume(agent) => {
            let Work::Session { session, flags, .. } = &review.work else {
                return Ok(None);
            };
            let cwd = review
                .cwd
                .as_deref()
                .and_then(|cwd| remap(cwd, &review.checkout, path))
                .unwrap_or_else(|| path.to_owned());
            let route = Route {
                source: from,
                destination: &review.destination.host,
                from: &review.checkout,
                to: path,
                cwd: &cwd,
            };
            Some(
                match resume_session(*agent, session, flags, &route, cancelled) {
                    Ok(argv) => argv,
                    Err(Error::Cancelled) => return Err(Error::Cancelled),
                    Err(error) => {
                        warnings.push(Warning::AgentStartedAfresh {
                            agent: *agent,
                            source: error,
                        });
                        agent.start_argv(None)
                    }
                },
            )
        }
        Action::Handoff { note, to: agent } => {
            let note = format!("{path}/{note}");
            Some(agent.start_argv(Some(&handoff_resume_prompt(&note))))
        }
        Action::Start(argv) | Action::Run(argv) => Some(remap_argv(argv, &review.checkout, path)),
    })
}

fn resume_session(
    agent: AgentKind,
    session: &super::snapshot::AgentSession,
    flags: &[String],
    route: &Route<'_>,
    cancelled: &AtomicBool,
) -> Result<Vec<String>> {
    let reference = move_session(agent, session, route, cancelled)?;
    Ok(agent.resume_argv(&reference, flags))
}

/// A review of an idle shell into a new worktree on `destination`, for
/// window tests that need a move ready to start.
#[cfg(any(test, feature = "test-support"))]
pub fn shell_review(destination: Place) -> Review {
    Review::Repository(Box::new(RepositoryReview {
        destination,
        repository: Repository {
            key: "/repo/.git".into(),
            label: "repo".into(),
            workspace_id: "w1".into(),
        },
        checkout: "/repo".into(),
        state: git::SourceState::default(),
        landing: Landing::Worktree { branch: None },
        cwd: None,
        work: Work::Shell,
        action: Action::Shell,
    }))
}

// Host scripts need /bin/sh; carrying is not offered on other clients.
#[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
