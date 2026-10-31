//! What the Teleport dialog says about a move, from `teleport.ftl`.
//! `herdr_remote::teleport` keeps its English labels for logs and errors.

use crate::i18n::fl;
use crate::teleport::{
    Step,
    job::{Action, Candidate, Destination},
    launch::{Work, command_line},
    provision::Arrival,
    remote::MatchReason,
};

/// The review's first line: where the worktree goes, and how.
pub(super) fn destination(candidate: &Candidate) -> String {
    let host = candidate.place.label.as_str();
    match &candidate.destination {
        Destination::Open { repository, reason } => {
            let repository = repository.label.as_str();
            match reason {
                MatchReason::Origin => {
                    fl!("teleport-to-origin", host = host, repository = repository)
                }
                MatchReason::Remote => {
                    fl!("teleport-to-remote", host = host, repository = repository)
                }
                MatchReason::Name => fl!("teleport-to-name", host = host, repository = repository),
            }
        }
        Destination::Reclaim { repository, .. } => fl!(
            "teleport-to-reclaim",
            host = host,
            repository = repository.label.as_str()
        ),
        Destination::Arrive(Arrival::Existing { path, .. }) => {
            fl!("teleport-to-existing", host = host, path = path.as_str())
        }
        Destination::Arrive(Arrival::Clone { path }) => match candidate.origin {
            Some(_) => fl!("teleport-to-clone", host = host, path = path.as_str()),
            None => fl!("teleport-to-copy", host = host, path = path.as_str()),
        },
    }
}

/// What a pane will run on the destination, as the review lists it.
pub(super) fn pane(work: &Work, action: &Action) -> String {
    // i18n: data
    let program = work.program().unwrap_or("shell");
    match action {
        Action::Shell => fl!("teleport-pane-shell"),
        Action::Resume(agent) => fl!("teleport-pane-resume", agent = agent.binary()),
        Action::Handoff { to, .. } if to.binary() == program => {
            fl!("teleport-pane-handoff-again", program = program)
        }
        Action::Handoff { to, .. } => fl!(
            "teleport-pane-handoff-to",
            program = program,
            agent = to.binary()
        ),
        Action::Start(argv) => fl!("teleport-pane-start", command = command_line(argv)),
        Action::Run(argv) => fl!("teleport-pane-run", command = command_line(argv)),
        Action::Missing(program) => fl!("teleport-pane-missing", program = program.as_str()),
    }
}

/// The step a move is on, or that it is starting.
pub(super) fn step(step: Option<Step>) -> String {
    let Some(step) = step else {
        return fl!("teleport-step-starting");
    };
    match step {
        Step::Discover => fl!("teleport-step-discover"),
        Step::Review => fl!("teleport-step-review"),
        Step::Clone => fl!("teleport-step-clone"),
        Step::Open => fl!("teleport-step-open"),
        Step::Handoff => fl!("teleport-step-handoff"),
        Step::Capture => fl!("teleport-step-capture"),
        Step::Transfer => fl!("teleport-step-transfer"),
        Step::Fetch => fl!("teleport-step-fetch"),
        Step::CreateWorktree => fl!("teleport-step-create-worktree"),
        Step::Restore => fl!("teleport-step-restore"),
        Step::Tabs => fl!("teleport-step-tabs"),
        Step::Retire => fl!("teleport-step-retire"),
        Step::Credentials => fl!("teleport-step-credentials"),
        Step::Sessions => fl!("teleport-step-sessions"),
        Step::Launch => fl!("teleport-step-launch"),
    }
}
