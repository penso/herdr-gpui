use super::*;

#[test]
fn the_github_cli_is_read_outside_the_credential_lock() {
    let profile = restore(Store::Environment, &Account::Main, true, || {
        // Another window's credential work must not wait on `gh`.
        let (tx, rx) = mpsc::sync_channel(1);
        thread::spawn(move || {
            let _ = tx.send(transaction(|| Ok(())).is_ok());
        });
        assert!(rx.recv_timeout(Duration::from_secs(5)).unwrap());
        Ok(Profile {
            login: "cli-user".into(),
            avatar_url: String::new(),
            token: std::sync::Arc::new("gho_fixture".into()),
            source: crate::Source::GitHubCli { repo_scope: true },
            organizations: None,
            access: Default::default(),
        })
    })
    .unwrap()
    .unwrap();
    assert!(profile.source.github_cli());
}

#[test]
fn only_the_main_account_reads_the_github_cli() {
    let host = Account::host("0123456789abcdef0123456789abcdef").unwrap();
    // Even a marker under a host's own entry is refused, never read with
    // this computer's `gh`.
    assert!(matches!(
        restore(Store::Environment, &host, true, || panic!(
            "must not read gh"
        )),
        Err(Error::Token)
    ));
    // Without the choice there is no store to hold a marker, so no `gh` either.
    assert!(
        restore(Store::Environment, &Account::Main, false, || panic!(
            "must not read gh"
        ))
        .unwrap()
        .is_none()
    );
}
