//! `herdr_orchestrator`'s errors from `error-orchestrator.ftl`: its
//! records and database, the agents it starts, its private files, and
//! GitLab. GitHub failures go through `herdr_github`'s mapping; database,
//! parser, Git, and GitLab text stays a placeable.

use super::Localize;
use crate::i18n::fl;
use herdr_orchestrator::{Error, gitlab, private_file};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            Self::NoDataDirectory => fl!("error-orchestrator-no-data-directory"),
            Self::Private(error) => error.localize(),
            Self::Database(error) => fl!("error-orchestrator-database", error = error.to_string()),
            Self::Json { column, source } => fl!(
                "error-orchestrator-json",
                column = *column,
                error = source.to_string()
            ),
            Self::Time { column, value, .. } => fl!(
                "error-orchestrator-time",
                column = *column,
                value = format!("{value:?}")
            ),
            Self::Provider(provider) => {
                fl!(
                    "error-orchestrator-provider",
                    provider = format!("{provider:?}")
                )
            }
            Self::ItemKey(key) => fl!("error-orchestrator-item-key", key = format!("{key:?}")),
            Self::Owner(owner) => fl!("error-orchestrator-owner", owner = format!("{owner:?}")),
            Self::NewerSchema { found, known } => fl!(
                "error-orchestrator-newer-schema",
                found = *found,
                known = *known
            ),
            Self::NotOwner(run) => fl!("error-orchestrator-not-owner", run = run.as_str()),
            Self::RunNotFound(run) => fl!("error-orchestrator-run-not-found", run = run.as_str()),
            Self::Script { operation, source } => fl!(
                "error-orchestrator-failed",
                kind = step(operation),
                operation = *operation,
                error = source.localize()
            ),
            Self::Git { operation, source } => fl!(
                "error-orchestrator-failed",
                kind = step(operation),
                operation = *operation,
                error = source.to_string()
            ),
            Self::Output { operation } => fl!(
                "error-orchestrator-output",
                kind = step(operation),
                operation = *operation
            ),
            Self::OutputJson { operation, source } => fl!(
                "error-orchestrator-output-json",
                kind = step(operation),
                operation = *operation,
                error = source.to_string()
            ),
            Self::UnsupportedHost => fl!("error-orchestrator-unsupported-host"),
            Self::InvalidCheckout => fl!("error-orchestrator-invalid-checkout"),
            Self::NotARepository => fl!("error-orchestrator-not-a-repository"),
            Self::RemoteUrl(url) => fl!("error-orchestrator-remote-url", url = format!("{url:?}")),
            Self::BeadsMissing => fl!("error-orchestrator-beads-missing"),
            Self::BeadId(id) => fl!("error-orchestrator-bead-id", id = format!("{id:?}")),
            Self::Github(error) => error.localize(),
            Self::Gitlab(error) => error.localize(),
            Self::Cancelled => fl!("error-orchestrator-cancelled"),
            Self::Worker(error) => fl!("error-orchestrator-worker", error = error.to_string()),
            Self::Busy => fl!("error-orchestrator-busy"),
            Self::NothingToOpen => fl!("error-orchestrator-nothing-to-open"),
            Self::WorkspaceClosed => fl!("error-orchestrator-workspace-closed"),
            Self::MenuOpen => fl!("error-orchestrator-menu-open"),
            Self::NotWebAddress => fl!("error-orchestrator-not-web-address"),
            Self::HostNotShown => fl!("error-orchestrator-host-not-shown"),
            Self::HostCannotTake => fl!("error-orchestrator-host-cannot-take"),
            Self::Teleport(error) => fl!("error-orchestrator-teleport", error = error.localize()),
            Self::PullRequestChanged => fl!("error-orchestrator-pull-request-changed"),
        }
    }
}

/// The message key for an orchestrator step the crate words in English;
/// one not listed is shown as it is.
fn step(operation: &str) -> &'static str {
    match operation {
        // keep-sorted start
        "Creating the worktree" => "create-worktree", // i18n: data
        "Discovering installed agents" => "discover-agents", // i18n: data
        "Naming the agent pane" => "name-pane",       // i18n: data
        "Naming the agent tab" => "name-tab",         // i18n: data
        "Opening the repository" => "open-repository", // i18n: data
        "Reading bd's options" => "read-bd-options",  // i18n: data
        "Reading Beads blockers" => "read-beads-blockers", // i18n: data
        "Reading Beads" => "read-beads",              // i18n: data
        "Reading GitHub" => "read-github",            // i18n: data
        "Reading pull request checks" => "read-checks", // i18n: data
        "Reading the base commit" => "read-base-commit", // i18n: data
        "Reading the branches" => "read-branches",    // i18n: data
        "Reading the repository" => "read-repository", // i18n: data
        "Reading the workspaces" => "read-workspaces", // i18n: data
        "Removing the worktree" => "remove-worktree", // i18n: data
        "Resolving the Beads folder" => "resolve-beads-folder", // i18n: data
        "Sending the message" => "send-message",      // i18n: data
        "Sending the prompt" => "send-prompt",        // i18n: data
        "Starting the agent" => "start-agent",        // i18n: data
        "Stopping the agent" => "stop-agent",         // i18n: data
        // keep-sorted end
        _ => "other",
    }
}

impl Localize for private_file::Error {
    fn localize(&self) -> String {
        match self {
            Self::Prepare { path, source } => fl!(
                "error-orchestrator-private-prepare",
                path = path.display().to_string(),
                error = source.to_string()
            ),
            Self::NotPrivate { path } => fl!(
                "error-orchestrator-private-not-private",
                path = path.display().to_string()
            ),
        }
    }
}

impl Localize for gitlab::Error {
    fn localize(&self) -> String {
        match self {
            Self::Network { origin, source } => fl!(
                "error-orchestrator-gitlab-network",
                origin = origin.as_str(),
                error = source.to_string()
            ),
            Self::Read(error) => fl!("error-orchestrator-gitlab-read", error = error.to_string()),
            Self::Size => fl!("error-orchestrator-gitlab-size"),
            Self::Json(error) => fl!("error-orchestrator-gitlab-json", error = error.to_string()),
            Self::Address(host) => {
                fl!(
                    "error-orchestrator-gitlab-address",
                    host = format!("{host:?}")
                )
            }
            Self::Token => fl!("error-orchestrator-gitlab-token"),
            Self::SignIn { repository } => fl!(
                "error-orchestrator-gitlab-sign-in",
                repository = repository.as_str()
            ),
            Self::InsecureRemote { repository } => fl!(
                "error-orchestrator-gitlab-insecure-remote",
                repository = repository.as_str()
            ),
            Self::Authentication => fl!("error-orchestrator-gitlab-authentication"),
            Self::EnvironmentRefused => fl!("error-orchestrator-gitlab-environment-refused"),
            Self::EnvrcRefused => fl!("error-orchestrator-gitlab-envrc-refused"),
            Self::Scope => fl!("error-orchestrator-gitlab-scope"),
            Self::Forbidden { repository } => fl!(
                "error-orchestrator-gitlab-forbidden",
                repository = repository.as_str()
            ),
            Self::NotFound { repository } => fl!(
                "error-orchestrator-gitlab-not-found",
                repository = repository.as_str()
            ),
            Self::RateLimit => fl!("error-orchestrator-gitlab-rate-limit"),
            Self::Status(status) => fl!("error-orchestrator-gitlab-status", status = *status),
            Self::Denied => fl!("error-orchestrator-gitlab-denied"),
            Self::CodeExpired => fl!("error-orchestrator-gitlab-code-expired"),
            Self::Host(host) => fl!("error-orchestrator-gitlab-host", host = format!("{host:?}")),
            Self::InsecureHost(host) => {
                fl!(
                    "error-orchestrator-gitlab-insecure-host",
                    host = host.as_str()
                )
            }
            Self::TooManyHosts => fl!(
                "error-orchestrator-gitlab-too-many-hosts",
                limit = gitlab::saved::MAX_HOSTS
            ),
            Self::Record => fl!("error-orchestrator-gitlab-record"),
            Self::Directory(error) => {
                fl!(
                    "error-orchestrator-gitlab-directory",
                    error = error.localize()
                )
            }
            #[cfg(target_os = "linux")]
            Self::NoKeyring(_) => fl!("error-orchestrator-gitlab-no-keyring"),
            Self::NoStore(_) => fl!("error-orchestrator-gitlab-no-store"),
            Self::Store(error) => fl!("error-orchestrator-gitlab-store", error = error.localize()),
        }
    }
}
