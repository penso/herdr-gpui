#![allow(clippy::unwrap_used)]
use super::*;
use crate::github::{device::repo_scope, token::Saved};
use secrecy::ExposeSecret;

#[test]
fn the_token_is_the_one_line_shaped_like_one() {
    assert_eq!(
        parse(true, b"gho_fixture\n").unwrap().expose_secret(),
        "gho_fixture"
    );
    assert_eq!(
        parse(true, b"warning: config moved\r\ngho_fixture\r\n")
            .unwrap()
            .expose_secret(),
        "gho_fixture"
    );
    assert!(matches!(
        parse(true, b""),
        Err(crate::Error::GitHub(Error::CliSignedOut))
    ));
    assert!(matches!(
        parse(true, b"gho_one\ngho_two\n"),
        Err(crate::Error::GitHub(Error::Token))
    ));
    // `gh` exits nonzero without a token for the host, whatever it printed.
    assert!(matches!(
        parse(false, b"no oauth token found for github.com\n"),
        Err(crate::Error::GitHub(Error::CliSignedOut))
    ));
}

#[test]
fn process_failures_are_told_in_terms_of_the_github_cli() {
    let missing = crate::pull_request::Error::Process {
        operation: "launch Git (install git on PATH)",
        source: std::io::ErrorKind::NotFound.into(),
    }
    .into();
    assert!(matches!(process_error(missing), Error::CliMissing));
    assert!(matches!(
        process_error(crate::pull_request::Error::Timeout.into()),
        Error::CliTimeout
    ));
    let error = process_error(crate::pull_request::Error::Size.into());
    assert!(
        matches!(&error, Error::CliProcess(source) if matches!(**source, crate::Error::PullRequest(crate::pull_request::Error::Size)))
    );
    assert!(std::error::Error::source(&error).is_some());
}

#[test]
fn only_a_missing_repo_scope_is_reported() {
    assert!(repo_scope(None));
    assert!(repo_scope(Some("gist, read:org, repo, workflow")));
    assert!(!repo_scope(Some("gist, read:org, public_repo")));
    assert!(!repo_scope(Some("")));
}

#[test]
fn the_saved_marker_is_never_a_token() {
    assert!(matches!(
        Saved::decode(&crate::github::token::GITHUB_CLI.into()).unwrap(),
        Saved::GitHubCli
    ));
    assert!(matches!(
        Saved::decode(&"gho_saved".into()).unwrap(),
        Saved::Token(_)
    ));
    // Any other JSON is a credential record, and must be a valid one.
    assert!(Saved::decode(&r#"{"version":1,"source":"other"}"#.into()).is_err());
}

#[cfg(unix)]
mod process {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A stand-in `gh` that checks its arguments and prints `body`'s output.
    fn fake(body: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gh");
        crate::test_executable::write(
            &path,
            format!(
                "#!/bin/sh\n[ \"$*\" = 'auth token --hostname github.com' ] || exit 3\n{body}\n"
            ),
            0o700,
        )
        .unwrap();
        (dir, path)
    }

    #[test]
    fn reads_the_clis_own_token_without_environment_tokens() {
        let (_dir, gh) = fake("printf '%s\\n' \"${GH_TOKEN:-${GITHUB_TOKEN:-gho_saved}}\"");
        let mut command = Command::new(&gh);
        command
            .env("GH_TOKEN", "env_token")
            .env("GITHUB_TOKEN", "env_token");
        assert_eq!(run(&mut command).unwrap().expose_secret(), "gho_saved");
    }

    #[test]
    fn signed_out_and_missing_clis_are_actionable() {
        let (_dir, gh) = fake("echo 'no oauth token found for github.com' >&2; exit 1");
        assert!(matches!(
            run(&mut Command::new(&gh)),
            Err(crate::Error::GitHub(Error::CliSignedOut))
        ));
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            run(&mut Command::new(dir.path().join("gh"))),
            Err(crate::Error::GitHub(Error::CliMissing))
        ));
    }

    #[test]
    fn the_private_file_keeps_the_marker_and_never_a_token() {
        use crate::github::{credentials, token::GITHUB_CLI};
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let name = c"github-credentials";
        credentials::store(dir.path(), name, Some(&GITHUB_CLI.into()), true).unwrap();
        let saved = credentials::read(dir.path(), name).unwrap().unwrap();
        assert_eq!(saved.expose_secret(), GITHUB_CLI);
        assert!(matches!(Saved::decode(&saved).unwrap(), Saved::GitHubCli));
        credentials::store(dir.path(), name, None, false).unwrap();
        assert!(credentials::read(dir.path(), name).unwrap().is_none());
    }
}
