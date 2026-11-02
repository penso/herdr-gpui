//! A Teleport dialog at each stage, with what the stage shows as data.
use super::*;
use crate::teleport::{Host, Node, Repository, SourceState, TabPlan, launch::Work};
use herdr_client::ConnectTarget;

mod en_xa;

fn place(label: &str) -> Place {
    Place {
        endpoint_id: label.into(),
        label: label.into(),
        host: Host::new(&ConnectTarget::Local).unwrap(),
    }
}

fn source() -> Source {
    Source {
        place: place("laptop"),
        workspace_id: "w1".into(),
        custom_label: None,
        repo_key: "repo".into(),
        repo_label: "herdr".into(),
        branch: Some("feature".into()),
        tab_labels: Default::default(),
    }
}

fn candidate() -> Candidate {
    Candidate {
        place: place("buildbox"),
        destination: job::Destination::Open {
            repository: Repository {
                key: "repo".into(),
                label: "herdr".into(),
                workspace_id: "w9".into(),
            },
            reason: MatchReason::Name,
        },
        origin: None,
    }
}

fn review(github: GitHubAccess) -> Review {
    let pane = |id: &str, action: Action| job::PanePlan {
        pane_id: id.into(),
        cwd: None,
        work: Work::Shell,
        action,
    };
    Review {
        checkout: "/src/herdr".into(),
        branch: "feature".into(),
        state: SourceState {
            branch: Some("feature".into()),
            head: "abc".into(),
            unpushed: 1,
            changed: 2,
            untracked: 3,
        },
        tabs: vec![
            TabPlan {
                label: None,
                tree: Node::Pane("p1".into()),
                panes: vec![
                    pane("p1", Action::Shell),
                    pane("p2", Action::Run(vec!["cargo".into(), "test".into()])),
                ],
            },
            TabPlan {
                label: Some("agents".into()),
                tree: Node::Pane("p3".into()),
                panes: vec![pane("p3", Action::Missing("aider".into()))],
            },
        ],
        reason: Some(MatchReason::Name),
        github,
    }
}

/// The dialog in each stage it shows.
pub(super) fn stages() -> Vec<(&'static str, Teleport)> {
    let at = |stage: Stage| {
        let mut teleport = Teleport::start(source(), "feature".into(), Vec::new());
        teleport.stage = stage;
        teleport
    };
    let mut choosing = Teleport::start(source(), "feature".into(), Vec::new());
    choosing.hosts = vec![
        HostRepositories {
            place: place("buildbox"),
            repositories: Some(Vec::new()),
            retired: Vec::new(),
        },
        HostRepositories {
            place: place("nas"),
            repositories: None,
            retired: Vec::new(),
        },
    ];
    vec![
        ("no hosts", at(Stage::Choosing)),
        ("choosing", choosing),
        ("reviewing", at(Stage::Reviewing(place("buildbox")))),
        (
            "ready, with a token",
            at(Stage::Ready(
                candidate(),
                Box::new(review(GitHubAccess::Token)),
            )),
        ),
        (
            "ready, without GitHub",
            at(Stage::Ready(
                candidate(),
                Box::new(review(GitHubAccess::Unavailable)),
            )),
        ),
        ("starting", at(Stage::Moving(candidate(), None))),
        ("moving", at(Stage::Moving(candidate(), Some(Step::Fetch)))),
    ]
}

/// The host and worktree names the stages show.
pub(super) const DATA: &[&str] = &["laptop", "buildbox", "nas", "feature", "herdr"];
