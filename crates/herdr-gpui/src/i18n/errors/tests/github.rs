use super::*;
use herdr_github::{AccountsError, Error, cli, pr_actions, pull_request};
use std::io;

fn io() -> io::Error {
    io::Error::other("pipe closed")
}

fn utf8() -> std::str::Utf8Error {
    String::from_utf8(vec![0xff]).unwrap_err().utf8_error()
}

#[test]
fn every_variant_has_a_message() {
    let mut errors = vec![
        Error::Authentication,
        Error::Authorization,
        Error::ClientIdEncoding,
        Error::Denied,
        Error::Device,
        Error::Encoding(utf8()),
        Error::Expired,
        Error::Forbidden,
        Error::Header(ureq::http::HeaderValue::from_str("\n").unwrap_err()),
        Error::InvalidClientId("github.oauth_client_id"),
        Error::json(serde_json::from_str::<u8>("\"token\"").unwrap_err()),
        Error::Network(ureq::Error::HostNotFound),
        Error::Query,
        Error::RateLimit { reset: None },
        Error::RateLimit {
            reset: Some(chrono::Utc::now()),
        },
        Error::Read(io()),
        Error::Rejected("Pull request is not mergeable".into()),
        Error::Size,
        Error::Status(502),
        Error::Token,
        Error::TokenType,
        Error::Cli(cli::Error::Process(Box::new(Error::Query))),
        Error::PrActions(pr_actions::Error::Busy),
        Error::PullRequest(pull_request::Error::Ambiguous),
        Error::Accounts(AccountsError::Id),
    ];
    errors.extend([true, false].map(|app_token| Error::RepositoryHidden {
        repository: "octo/repo".into(),
        app_token,
    }));
    errors.extend(["authentication", "list", "profile", "upload"].map(Error::Worker));
    assert_localized(&errors);
    assert_localized(&[
        Error::Client(herdr_client::Error::Disconnected),
        Error::Credentials(herdr_credentials::Error::Permissions),
        Error::Path(herdr_client::local_path::PathError::MissingHome),
    ]);
}

#[test]
fn json_keeps_the_parser_diagnostic_out() {
    let error = Error::json(serde_json::from_str::<u8>("\"ghp_secret\"").unwrap_err());
    assert!(!error.localize().contains("ghp_secret"));
    assert!(!error.localize().contains("line"));
}

#[test]
fn every_accounts_variant_has_a_message() {
    assert_localized(&[
        AccountsError::TooMany(16),
        AccountsError::Id,
        AccountsError::DuplicateId("0123456789abcdef0123456789abcdef".into()),
        AccountsError::NotAList,
        AccountsError::Owner("not a name".into()),
        AccountsError::Random(getrandom::Error::UNSUPPORTED),
    ]);
}

#[test]
fn every_cli_variant_has_a_message() {
    assert_localized(&[
        cli::Error::Missing,
        cli::Error::Process(Box::new(Error::Query)),
        cli::Error::Rejected,
        cli::Error::SignedOut,
        cli::Error::Timeout,
    ]);
}

#[test]
fn every_pull_request_variant_has_a_message() {
    let mut errors = vec![
        pull_request::Error::AbsolutePath,
        pull_request::Error::Ambiguous,
        pull_request::Error::AmbiguousWorktree,
        pull_request::Error::Branch,
        pull_request::Error::BranchChanged,
        pull_request::Error::Cancelled,
        pull_request::Error::Checkout,
        pull_request::Error::Encoding(utf8()),
        pull_request::Error::Identity,
        pull_request::Error::Metadata,
        pull_request::Error::MissingWorktree,
        pull_request::Error::Origin,
        pull_request::Error::Repository,
        pull_request::Error::RepositoryMismatch,
        pull_request::Error::Size,
        pull_request::Error::Timeout,
        pull_request::Error::UntrustedEndpoint,
        pull_request::Error::WorkspaceRepository,
        pull_request::Error::WorktreeLookup,
    ];
    errors.extend(
        [
            "configure process errors",
            "configure process output",
            "create process output channel",
            "launch Git (install git on PATH)",
            "read process output",
            "wait for process",
            "read the moon",
        ]
        .map(|operation| pull_request::Error::Process {
            operation,
            source: io(),
        }),
    );
    assert_localized(&errors);
}

#[test]
fn every_pr_actions_variant_has_a_message() {
    assert_localized(&[
        pr_actions::Error::Busy,
        pr_actions::Error::CommentBody,
        pr_actions::Error::MergeChanged,
        pr_actions::Error::MergeMethod,
        pr_actions::Error::Process { source: io() },
        pr_actions::Error::Target,
        pr_actions::Error::Worker,
    ]);
}
