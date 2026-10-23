use super::*;
use crate::github::{Account, Source};

fn cli_profile(repo_scope: bool) -> Profile {
    Profile {
        login: "cli-user".into(),
        avatar: None,
        token: Arc::new("gho_fixture".into()),
        avatar_updates: None,
        source: Source::GitHubCli { repo_scope },
        organizations: None,
        access: Default::default(),
    }
}

/// Wait for the worker `launch` started and hand its reply back to `poll`.
fn finish(auth: &mut Auth) {
    let reply = auth
        .incoming
        .take()
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    deliver(auth, reply);
}

fn connected_with_cli() -> Auth {
    let mut auth = Auth::default();
    auth.use_github_cli_with(Store::Keyring, || Ok(Reply::GitHubCli(cli_profile(true))));
    assert!(auth.busy());
    assert_eq!(
        auth.message.as_deref(),
        Some("Checking GitHub CLI sign-in...")
    );
    finish(&mut auth);
    assert!(auth.poll_with_store(|_| panic!("the worker already saved the marker")));
    auth
}

#[test]
fn choosing_the_cli_connects_and_marks_the_session() {
    let auth = connected_with_cli();
    assert!(auth.connected());
    assert!(!auth.failed);
    assert!(auth.message.is_none());
    assert!(auth.github_cli);
    assert!(auth.can_sign_out());
    assert_eq!(auth.store(), Store::Keyring);
    assert!(auth.profile.as_ref().unwrap().source.github_cli());
    assert!(auth.next_session_check.is_some());
}

#[test]
fn a_cli_failure_is_shown_and_leaves_nothing_to_sign_out() {
    let mut auth = Auth::default();
    auth.use_github_cli_with(Store::Keyring, || {
        Err(crate::Error::GitHub(Error::CliSignedOut))
    });
    auth.cancel(); // The worker may save the marker; dismissal waits for it.
    finish(&mut auth);
    auth.poll_with_store(|_| panic!("nothing to save"));
    assert!(!auth.connected());
    assert!(auth.failed);
    assert!(!auth.github_cli);
    assert!(!auth.can_sign_out());
    assert_eq!(
        auth.message.as_deref(),
        Some(
            crate::Error::GitHub(Error::CliSignedOut)
                .to_string()
                .as_str()
        )
    );
}

#[test]
fn a_host_account_never_uses_this_computers_cli() {
    let host = Account::host("0123456789abcdef0123456789abcdef").unwrap();
    let mut auth = Auth::for_account(host);
    auth.cli_available = Some((Instant::now(), true));
    assert!(!auth.github_cli_available());
    auth.use_github_cli_with(Store::Keyring, || panic!("must not read the GitHub CLI"));
    assert!(!auth.busy());
    auth.cli_available = None;
    auth.probe_github_cli_with(Instant::now(), || panic!("must not ask the GitHub CLI"));
    assert!(auth.cli_probe.is_none());
}

#[test]
fn the_probe_answers_once_and_is_not_repeated_while_fresh() {
    let mut auth = Auth::default();
    let now = Instant::now();
    auth.probe_github_cli_with(now, || true);
    let rx = auth.cli_probe.take().unwrap();
    let available = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let (tx, rx) = mpsc::sync_channel(1);
    tx.send(available).unwrap();
    auth.cli_probe = Some(rx);
    assert!(auth.poll_at(now, |_| panic!("no storage"), |_, _| panic!("no profile")));
    assert!(auth.github_cli_available());
    auth.probe_github_cli_with(now + Duration::from_secs(1), || panic!("still fresh"));
    assert!(auth.cli_probe.is_none());
    // A connected account has nothing to offer.
    let mut connected = connected_with_cli();
    connected.cli_available = None;
    connected.probe_github_cli_with(now, || panic!("already connected"));
    assert!(connected.cli_probe.is_none());
}

#[test]
fn stopping_the_cli_session_removes_only_the_marker() {
    let mut auth = connected_with_cli();
    auth.sign_out();
    assert!(!auth.github_cli);
    assert!(auth.poll_with_store(|token| {
        assert!(token.is_none());
        Ok(())
    }));
    finish(&mut auth);
    auth.poll_with_store(|_| panic!("removal already ran"));
    assert!(!auth.connected());
    assert!(
        auth.message
            .as_deref()
            .unwrap()
            .starts_with("Stopped using GitHub CLI authentication.")
    );
}

#[test]
fn signing_out_of_gh_ends_the_session_at_its_next_check() {
    for error in [
        crate::Error::GitHub(Error::CliSignedOut),
        crate::Error::GitHub(Error::CliRejected),
        crate::Error::GitHub(Error::CliMissing),
    ] {
        let mut auth = connected_with_cli();
        let now = Instant::now() + Duration::from_secs(6 * 60);
        let (tx, rx) = mpsc::sync_channel(1);
        assert!(auth.poll_at(
            now,
            |_| panic!("no storage"),
            move |token, _| {
                assert!(token.is_none());
                let _ = rx.recv_timeout(Duration::from_secs(5));
                Err(error)
            }
        ));
        tx.send(()).unwrap();
        let rx = auth.profile_incoming.take().unwrap();
        let result = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        auth.complete_profile_fixture(result);
        auth.poll_at(now, |_| panic!("no storage"), |_, _| panic!("one check"));
        assert!(!auth.connected());
        assert!(auth.failed);
        assert!(auth.can_sign_out());
    }
    // A slow `gh` is not a sign-out: the session survives it.
    let mut auth = connected_with_cli();
    auth.complete_profile_fixture(Err(crate::Error::GitHub(Error::CliTimeout)));
    auth.poll_with_store(|_| panic!("no storage"));
    assert!(auth.connected());
    assert!(auth.failed);
}
