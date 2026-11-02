//! Teleport's failures, each script or output failure by the step it
//! happened in.

use crate::i18n::{errors::Localize, fl};
use herdr_remote::teleport::{Error, Step};

/// The step as a selector key: `error-remote-teleport-script` and
/// `-decode` write each step's sentence in full.
fn step(step: Step) -> &'static str {
    match step {
        Step::Discover => "discover",
        Step::Review => "review",
        Step::Clone => "clone",
        Step::Open => "open",
        Step::Handoff => "handoff",
        Step::Capture => "capture",
        Step::Transfer => "transfer",
        Step::Fetch => "fetch",
        Step::CreateWorktree => "create-worktree",
        Step::Restore => "restore",
        Step::Tabs => "tabs",
        Step::Retire => "retire",
        Step::Credentials => "credentials",
        Step::Sessions => "sessions",
        Step::Launch => "launch",
    }
}

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::Script { step: at, source } => fl!(
                "error-remote-teleport-script",
                step = step(*at),
                error = source.localize()
            ),
            Self::Decode { step: at, .. } => {
                fl!("error-remote-teleport-decode", step = step(*at))
            }
            Self::UnsupportedHost => fl!("error-remote-teleport-unsupported-host"),
            Self::DevelopmentSession => fl!("error-remote-teleport-development-session"),
            Self::WorkspaceGone => fl!("error-remote-teleport-workspace-gone"),
            Self::PaneGone => fl!("error-remote-teleport-pane-gone"),
            Self::RepositoryNotOpen { host } => {
                fl!(
                    "error-remote-teleport-repository-not-open",
                    host = host.as_str()
                )
            }
            Self::SourceMoved => fl!("error-remote-teleport-source-moved"),
            Self::OutsideCheckout { path } => {
                fl!(
                    "error-remote-teleport-outside-checkout",
                    path = path.as_str()
                )
            }
            Self::DestinationDirectoryMissing { path } => fl!(
                "error-remote-teleport-destination-directory-missing",
                path = path.as_str()
            ),
            Self::OutsideHome { path } => {
                fl!("error-remote-teleport-outside-home", path = path.as_str())
            }
            Self::SessionAgentWorking => fl!("error-remote-teleport-session-agent-working"),
            Self::SessionCwdUnavailable => fl!("error-remote-teleport-session-cwd-unavailable"),
            Self::SessionNotStarted { agent, host } => fl!(
                "error-remote-teleport-session-not-started",
                agent = *agent,
                host = host.as_str()
            ),
            Self::SessionProgramMissing { agent, host } => fl!(
                "error-remote-teleport-session-program-missing",
                agent = *agent,
                host = host.as_str()
            ),
            Self::SessionRequired => fl!("error-remote-teleport-session-required"),
            Self::SessionWorkspaceLeftBehind {
                workspace_id,
                cause,
            } => fl!(
                "error-remote-teleport-session-workspace-left-behind",
                error = cause.localize(),
                workspace = workspace_id.as_str()
            ),
            Self::SourceSessionChanged => fl!("error-remote-teleport-source-session-changed"),
            Self::WorkerStopped => fl!("error-remote-teleport-worker-stopped"),
            Self::SpaceGone { host } => {
                fl!("error-remote-teleport-space-gone", host = host.as_str())
            }
            Self::SpaceChanged { path } => {
                fl!("error-remote-teleport-space-changed", path = path.as_str())
            }
            Self::SpaceDiverged { branch } => {
                fl!(
                    "error-remote-teleport-space-diverged",
                    branch = branch.as_str()
                )
            }
            Self::LeftBehind { path, cause } => fl!(
                "error-remote-teleport-left-behind",
                error = cause.localize(),
                path = path.as_str()
            ),
            Self::NoCommit { reference } => fl!(
                "error-remote-teleport-no-commit",
                reference = reference.as_str()
            ),
            Self::DetachedHead => fl!("error-remote-teleport-detached-head"),
            Self::BranchDiverged { branch } => {
                fl!(
                    "error-remote-teleport-branch-diverged",
                    branch = branch.as_str()
                )
            }
            Self::BranchCheckedOut { branch, path } => fl!(
                "error-remote-teleport-branch-checked-out",
                branch = branch.as_str(),
                path = path.as_str()
            ),
            Self::SessionMissing { agent } => {
                fl!("error-remote-teleport-session-missing", agent = *agent)
            }
            Self::LocalFile(_) => fl!("error-remote-teleport-local-file"),
            Self::InvalidToken => fl!("error-remote-teleport-invalid-token"),
            Self::Cancelled => fl!("error-remote-teleport-cancelled"),
        }
    }
}
