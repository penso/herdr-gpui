use super::*;
use herdr_orchestrator::{Error, gitlab, private_file};
use std::{io, path::PathBuf};

fn io() -> io::Error {
    io::Error::other("pipe closed")
}

fn json() -> serde_json::Error {
    serde_json::from_str::<u8>("x").unwrap_err()
}

/// The steps the crate names, and one it does not.
const STEPS: &[&str] = &[
    "Creating the worktree",
    "Discovering installed agents",
    "Naming the agent pane",
    "Naming the agent tab",
    "Opening the repository",
    "Reading Beads blockers",
    "Reading Beads",
    "Reading GitHub",
    "Reading bd's options",
    "Reading pull request checks",
    "Reading the base commit",
    "Reading the branches",
    "Reading the repository",
    "Reading the workspaces",
    "Removing the worktree",
    "Resolving the Beads folder",
    "Sending the message",
    "Sending the prompt",
    "Starting the agent",
    "Stopping the agent",
    "Reading the moon",
];

/// `rusqlite` is not the app's dependency, so its error comes from the
/// orchestrator's own store opening a file that is not a database.
fn database() -> Error {
    let root = std::env::temp_dir().join(format!("herdr-i18n-db-{}", std::process::id()));
    let path = root.join("private").join("state.sqlite");
    private_file::prepare(&path).unwrap();
    std::fs::write(&path, [b'x'; 4096]).unwrap();
    let error = herdr_orchestrator::Store::open(&path).err().unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    assert!(matches!(error, Error::Database(_)), "{error}");
    error
}

#[test]
fn every_variant_has_a_message() {
    let mut errors = vec![
        Error::NoDataDirectory,
        Error::Private(private_file::Error::NotPrivate {
            path: PathBuf::from("/tmp/state"),
        }),
        database(),
        Error::Json {
            column: "labels",
            source: json(),
        },
        Error::Time {
            column: "updated_at",
            value: "yesterday".into(),
            source: chrono::NaiveDate::parse_from_str("x", "%Y-%m-%d").unwrap_err(),
        },
        Error::Provider("jira".into()),
        Error::ItemKey("github:".into()),
        Error::Owner("nobody".into()),
        Error::NewerSchema { found: 9, known: 7 },
        Error::NotOwner("run-1".into()),
        Error::RunNotFound("run-2".into()),
        Error::Git {
            operation: "Reading the repository",
            source: Box::new(io()),
        },
        Error::UnsupportedHost,
        Error::InvalidCheckout,
        Error::NotARepository,
        Error::RemoteUrl("file:///".into()),
        Error::BeadsMissing,
        Error::BeadId("bad id".into()),
        Error::Github(Box::new(herdr_github::Error::Query)),
        Error::Gitlab(gitlab::Error::Size),
        Error::Cancelled,
        Error::Worker(io()),
        Error::Busy,
        Error::NothingToOpen,
        Error::WorkspaceClosed,
        Error::MenuOpen,
        Error::NotWebAddress,
        Error::HostNotShown,
        Error::HostCannotTake,
        Error::Teleport(herdr_remote::teleport::Error::UnsupportedHost),
        Error::PullRequestChanged,
    ];
    for operation in STEPS {
        errors.extend([
            Error::Script {
                operation,
                source: herdr_client::Error::Disconnected,
            },
            Error::Output { operation },
            Error::OutputJson {
                operation,
                source: json(),
            },
        ]);
    }
    assert_localized(&errors);
}

#[test]
fn every_private_file_variant_has_a_message() {
    let path = PathBuf::from("/tmp/state.sqlite");
    assert_localized(&[
        private_file::Error::Prepare {
            path: path.clone(),
            source: io(),
        },
        private_file::Error::NotPrivate { path },
    ]);
}

#[test]
fn every_gitlab_variant_has_a_message() {
    let repository = || "group/project".to_owned();
    assert_localized(&[
        gitlab::Error::Network {
            origin: "https://gitlab.example.com".into(),
            source: Box::new(ureq::Error::HostNotFound),
        },
        gitlab::Error::Read(io()),
        gitlab::Error::Size,
        gitlab::Error::Json(json()),
        gitlab::Error::Address("gitlab example".into()),
        gitlab::Error::Token,
        gitlab::Error::SignIn {
            repository: repository(),
        },
        gitlab::Error::InsecureRemote {
            repository: repository(),
        },
        gitlab::Error::Authentication,
        gitlab::Error::EnvironmentRefused,
        gitlab::Error::EnvrcRefused,
        gitlab::Error::Scope,
        gitlab::Error::Forbidden {
            repository: repository(),
        },
        gitlab::Error::NotFound {
            repository: repository(),
        },
        gitlab::Error::RateLimit,
        gitlab::Error::Status(500),
        gitlab::Error::Denied,
        gitlab::Error::CodeExpired,
        gitlab::Error::Host("not a host".into()),
        gitlab::Error::InsecureHost("http://gitlab.example.com".into()),
        gitlab::Error::TooManyHosts,
        gitlab::Error::Record,
        gitlab::Error::Directory(herdr_client::local_path::PathError::MissingHome),
        #[cfg(target_os = "linux")]
        gitlab::Error::NoKeyring(herdr_credentials::Error::Permissions),
        gitlab::Error::NoStore(herdr_credentials::Error::Permissions),
        gitlab::Error::Store(herdr_credentials::Error::Permissions),
    ]);
}
