use super::*;
use herdr_remote::teleport::{Error, Step};

const STEPS: [Step; 15] = [
    Step::Discover,
    Step::Review,
    Step::Clone,
    Step::Open,
    Step::Handoff,
    Step::Capture,
    Step::Transfer,
    Step::Fetch,
    Step::CreateWorktree,
    Step::Restore,
    Step::Tabs,
    Step::Retire,
    Step::Credentials,
    Step::Sessions,
    Step::Launch,
];

#[test]
fn every_step_has_its_sentence() {
    let errors: Vec<Error> = STEPS
        .into_iter()
        .flat_map(|step| {
            [
                Error::Script {
                    step,
                    source: client(),
                },
                Error::Decode {
                    step,
                    source: json(),
                },
            ]
        })
        .collect();
    assert_localized(&errors);
}

#[test]
fn every_variant_has_a_message() {
    let path = || "/home/me/project".to_owned();
    let host = || "build-box".to_owned();
    let branch = || "config-reload".to_owned();
    assert_localized(&[
        Error::UnsupportedHost,
        Error::DevelopmentSession,
        Error::WorkspaceGone,
        Error::PaneGone,
        Error::RepositoryNotOpen { host: host() },
        Error::SourceMoved,
        Error::OutsideCheckout { path: path() },
        Error::DestinationDirectoryMissing { path: path() },
        Error::OutsideHome { path: path() },
        Error::SessionAgentWorking,
        Error::SessionCwdUnavailable,
        Error::SessionNotStarted {
            agent: "Claude Code",
            host: host(),
        },
        Error::SessionProgramMissing {
            agent: "Codex",
            host: host(),
        },
        Error::SessionRequired,
        Error::SessionWorkspaceLeftBehind {
            workspace_id: "w-12".into(),
            cause: Box::new(Error::PaneGone),
        },
        Error::SourceSessionChanged,
        Error::WorkerStopped,
        Error::SpaceGone { host: host() },
        Error::SpaceChanged { path: path() },
        Error::SpaceDiverged { branch: branch() },
        Error::LeftBehind {
            path: path(),
            cause: Box::new(Error::DetachedHead),
        },
        Error::NoCommit {
            reference: "abc123".into(),
        },
        Error::DetachedHead,
        Error::BranchDiverged { branch: branch() },
        Error::BranchCheckedOut {
            branch: branch(),
            path: path(),
        },
        Error::SessionMissing {
            agent: "Claude Code",
        },
        Error::LocalFile(io()),
        Error::InvalidToken,
        Error::Cancelled,
    ]);
}
