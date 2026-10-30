//! `herdr_github`'s errors from `error-github.ftl`: sign-in and requests,
//! the `[github] accounts` list, the GitHub CLI, pull request lookups, and
//! pull request actions. GitHub's own text stays a placeable, and what the
//! crate's `Display` keeps out (a JSON parser's diagnostic, which can quote
//! a credential) stays out here too.

use super::Localize;
use crate::i18n::fl;
use chrono::{DateTime, DurationRound, Local, TimeDelta, Utc};
use herdr_github::{AccountsError, Error, cli, pr_actions, pull_request};

impl Localize for Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Accounts(error) => error.localize(),
            Self::Authentication => fl!("error-github-authentication"),
            Self::Authorization => fl!("error-github-authorization"),
            Self::Cli(error) => error.localize(),
            Self::Client(error) => error.localize(),
            Self::ClientIdEncoding => fl!("error-github-client-id-encoding"),
            Self::Credentials(error) => error.localize(),
            Self::Denied => fl!("error-github-denied"),
            Self::Device => fl!("error-github-device"),
            Self::Encoding(_) => fl!("error-github-encoding"),
            Self::Expired => fl!("error-github-expired"),
            Self::Forbidden => fl!("error-github-forbidden"),
            Self::Header(_) => fl!("error-github-header"),
            Self::InvalidClientId(key) => fl!("error-github-invalid-client-id", key = *key),
            // The parser's diagnostic is redacted, as `Display` redacts it.
            Self::Json(_) => fl!("error-github-json"),
            Self::Network(error) => fl!("error-github-network", error = error.to_string()),
            Self::Path(error) => error.localize(),
            Self::PrActions(error) => error.localize(),
            Self::PullRequest(error) => error.localize(),
            Self::Query => fl!("error-github-query"),
            Self::RateLimit { reset } => rate_limit(*reset),
            Self::Read(_) => fl!("error-github-read"),
            Self::Rejected(reason) => fl!("error-github-rejected", reason = reason.as_str()),
            Self::RepositoryHidden {
                repository,
                app_token,
            } => fl!(
                "error-github-repository-hidden",
                repository = repository.as_str(),
                sign_in = if *app_token { "app" } else { "token" }
            ),
            Self::Size => fl!("error-github-size"),
            Self::Status(status) => fl!("error-github-status", status = *status),
            Self::Token => fl!("error-github-token"),
            Self::TokenType => fl!("error-github-token-type"),
            Self::Worker(worker) => fl!(
                "error-github-worker",
                kind = match *worker {
                    "authentication" | "list" | "profile" => *worker,
                    _ => "other",
                },
                worker = *worker
            ),
            // keep-sorted end
        }
    }
}

/// When GitHub's quota refills, as `herdr_updater::retry_hint` words it:
/// the reset rounded up to the minute, in local time.
fn rate_limit(reset: Option<DateTime<Utc>>) -> String {
    let Some(reset) = reset else {
        return fl!("error-github-rate-limit");
    };
    let minute = reset
        .duration_round_up(TimeDelta::minutes(1))
        .unwrap_or(reset);
    let time = minute.with_timezone(&Local).format("%H:%M").to_string();
    fl!("error-github-rate-limit-until", time = time)
}

impl Localize for AccountsError {
    fn localize(&self) -> String {
        match self {
            Self::TooMany(limit) => fl!("error-github-accounts-too-many", limit = *limit),
            Self::Id => fl!("error-github-accounts-id"),
            Self::DuplicateId(id) => fl!("error-github-accounts-duplicate-id", id = id.as_str()),
            Self::NotAList => fl!("error-github-accounts-not-a-list"),
            Self::Owner(owner) => {
                fl!("error-github-accounts-owner", owner = format!("{owner:?}"))
            }
            Self::Random(_) => fl!("error-github-accounts-random"),
        }
    }
}

impl Localize for cli::Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Missing => fl!("error-github-cli-missing"),
            Self::Process(_) => fl!("error-github-cli-process"),
            Self::Rejected => fl!("error-github-cli-rejected"),
            Self::SignedOut => fl!("error-github-cli-signed-out"),
            Self::Timeout => fl!("error-github-cli-timeout"),
            // keep-sorted end
        }
    }
}

impl Localize for pull_request::Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::AbsolutePath => fl!("error-github-pull-request-absolute-path"),
            Self::Ambiguous => fl!("error-github-pull-request-ambiguous"),
            Self::AmbiguousWorktree => fl!("error-github-pull-request-ambiguous-worktree"),
            Self::Branch => fl!("error-github-pull-request-branch"),
            Self::BranchChanged => fl!("error-github-pull-request-branch-changed"),
            Self::Cancelled => fl!("error-github-pull-request-cancelled"),
            Self::Checkout => fl!("error-github-pull-request-checkout"),
            Self::Encoding(_) => fl!("error-github-pull-request-encoding"),
            Self::Identity => fl!("error-github-pull-request-identity"),
            Self::Metadata => fl!("error-github-pull-request-metadata"),
            Self::MissingWorktree => fl!("error-github-pull-request-missing-worktree"),
            Self::Origin => fl!("error-github-pull-request-origin"),
            Self::Process { operation, .. } => fl!(
                "error-github-pull-request-process",
                kind = lookup_step(operation),
                operation = *operation
            ),
            Self::Repository => fl!("error-github-pull-request-repository"),
            Self::RepositoryMismatch => fl!("error-github-pull-request-repository-mismatch"),
            Self::Size => fl!("error-github-pull-request-size"),
            Self::Timeout => fl!("error-github-pull-request-timeout"),
            Self::UntrustedEndpoint => fl!("error-github-pull-request-untrusted-endpoint"),
            Self::WorkspaceRepository => fl!("error-github-pull-request-workspace-repository"),
            Self::WorktreeLookup => fl!("error-github-pull-request-worktree-lookup"),
            // keep-sorted end
        }
    }
}

/// The message key for a pull request lookup's `operation`, which the
/// crate words in English; one it does not list is shown as it is.
fn lookup_step(operation: &str) -> &'static str {
    match operation {
        "configure process errors" => "configure-errors", // i18n: data
        "configure process output" => "configure-output", // i18n: data
        "create process output channel" => "create-channel", // i18n: data
        "launch Git (install git on PATH)" => "launch-git", // i18n: data
        "read process output" => "read-output",           // i18n: data
        "wait for process" => "wait",                     // i18n: data
        _ => "other",
    }
}

impl Localize for pr_actions::Error {
    fn localize(&self) -> String {
        match self {
            // keep-sorted start
            Self::Busy => fl!("error-github-pr-actions-busy"),
            Self::CommentBody => fl!("error-github-pr-actions-comment-body"),
            Self::MergeChanged => fl!("error-github-pr-actions-merge-changed"),
            Self::MergeMethod => fl!("error-github-pr-actions-merge-method"),
            Self::Process { .. } => fl!("error-github-pr-actions-process"),
            Self::Target => fl!("error-github-pr-actions-target"),
            Self::Worker => fl!("error-github-pr-actions-worker"),
            // keep-sorted end
        }
    }
}
