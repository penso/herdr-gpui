//! What the Teleport dialog and the pane menu's move to another device say
//! about a move, from `teleport.ftl`. `herdr_remote::teleport` keeps its
//! English labels for logs.

use crate::i18n::{errors::Localize, fl};
use crate::teleport::{
    Step, Warning,
    job::{Action, Candidate, Destination},
    launch::{Work, command_line},
    provision::Arrival,
    remote::MatchReason,
};

/// The review's first line: where the worktree goes, and how.
pub(crate) fn destination(candidate: &Candidate) -> String {
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
pub(crate) fn pane(work: &Work, action: &Action) -> String {
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
pub(crate) fn step(step: Option<Step>) -> String {
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

/// What did not carry over in a move that otherwise finished.
pub(crate) fn warning(warning: &Warning) -> String {
    match warning {
        Warning::AgentStartedAfresh { agent, source } => fl!(
            "teleport-warning-started-afresh",
            agent = agent.binary(),
            error = source.localize()
        ),
        Warning::EnvrcTracked => fl!("teleport-warning-envrc"),
        Warning::GitHubAccess { source } => {
            fl!("teleport-warning-github", error = source.localize())
        }
        Warning::NoHandoffNote {
            pane: Some(pane),
            source,
        } => fl!(
            "teleport-warning-no-note-from",
            pane = pane.as_str(),
            error = source.localize()
        ),
        Warning::NoHandoffNote { pane: None, source } => {
            fl!("teleport-warning-no-note", error = source.localize())
        }
        Warning::ProgramMissing { program, host } => fl!(
            "teleport-warning-missing",
            program = program.as_str(),
            host = host.as_str()
        ),
        Warning::SourcePaneOpen { source } => {
            fl!("teleport-warning-pane-open", error = source.localize())
        }
    }
}
