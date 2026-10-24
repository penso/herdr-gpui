use super::*;
use crate::store::resolve_token;

#[test]
fn github_sign_ins_follow_the_build_and_the_plaintext_opt_in() {
    let mut config = crate::config::GitHubConfig::default();
    #[cfg(target_os = "macos")]
    assert_eq!(
        store::select(&config),
        if herdr_credentials::RELEASE_BUILD {
            Store::Keyring
        } else {
            Store::File
        }
    );
    #[cfg(any(target_os = "linux", windows))]
    assert_eq!(store::select(&config), Store::Keyring);
    config.allow_plaintext_credentials = true;
    // macOS picks its store from the build alone and ignores the opt-in.
    #[cfg(target_os = "macos")]
    assert_eq!(
        store::select(&config),
        Store::choose(false, KEYRING, store::FILE_DEFAULT)
    );
    #[cfg(target_os = "linux")]
    assert_eq!(store::select(&config), Store::File);
    // Windows keeps using Credential Manager: the file cannot be private there.
    if !store::FILE {
        assert_eq!(store::select(&config), Store::choose(false, KEYRING, false));
    }
}
#[test]
fn credential_notes_state_where_tokens_are_kept() {
    assert!(store::note(Store::Environment, false).is_none());
    assert!(store::note(Store::Environment, true).is_none());
    assert!(matches!(
        store::note(Store::Keyring, false),
        Some(Note::Info(_))
    ));
    assert!(
        store::note(Store::Keyring, true).is_none(),
        "a connected account already proved keyring access"
    );
    for connected in [false, true] {
        let Some(Note::Warning(text)) = store::note(Store::File, connected) else {
            panic!("unencrypted storage must always warn");
        };
        assert!(text.starts_with("WARNING: "));
    }
}
fn load_fixture_profile(auth: &mut Auth, store: Store, profile: Option<Profile>) {
    assert!(auth.poll_with(
        |_| panic!("policy reload must not change stored credentials"),
        move |token, policy| {
            assert!(token.is_none(), "must resolve under the new policy");
            assert_eq!(policy, store);
            assert_eq!(thread::current().name(), Some("herdr-github-profile"));
            Ok(profile)
        },
    ));
    let result = auth
        .profile_incoming
        .take()
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let (tx, rx) = mpsc::sync_channel(1);
    tx.send(result).ok().unwrap();
    auth.profile_incoming = Some(rx);
    assert!(auth.poll_with(
        |_| panic!("profile does not persist tokens"),
        |_, _| panic!("no duplicate profile request"),
    ));
}

#[test]
fn enabling_plaintext_reloads_saved_token_but_explicit_signout_stays_suppressed() {
    let mut auth = Auth::default();
    // Drive the backend directly: which one a configuration selects depends on
    // the build, but every transition between them must behave the same way.
    assert!(auth.initialize_with(Store::Environment));
    load_fixture_profile(&mut auth, Store::Environment, None);
    assert!(!auth.connected());
    assert!(auth.initialize_with(Store::File));
    load_fixture_profile(&mut auth, Store::File, Auth::connected_fixture().profile);
    assert!(auth.connected());
    assert!(
        !auth.initialize_with(Store::File),
        "unchanged policy must not poll"
    );
    auth.sign_out();
    auth.poll_with(
        |token| {
            assert!(token.is_none());
            Ok(())
        },
        |_, _| panic!("signed out"),
    );
    let reply = auth
        .incoming
        .take()
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    deliver(&mut auth, reply);
    auth.poll_with(|_| panic!("already removed"), |_, _| panic!("signed out"));
    for store in [Store::Environment, Store::File, Store::Keyring] {
        assert!(!auth.initialize_with(store));
        assert_eq!(auth.store(), store);
        assert!(auth.signed_out);
        assert!(!auth.loading_profile());
        assert!(!auth.connected());
        assert!(!auth.poll_with(
            |_| panic!("no store access"),
            |_, _| panic!("no credential reload")
        ));
    }
}

#[test]
fn disabling_plaintext_clears_session_and_rejects_late_profile() {
    let mut auth = Auth::default();
    assert!(auth.initialize_with(Store::File));
    load_fixture_profile(&mut auth, Store::File, Auth::connected_fixture().profile);
    assert!(auth.connected());
    assert!(auth.initialize_with(Store::Environment));
    assert!(
        !auth.connected(),
        "old token cannot remain usable during reload"
    );
    assert!(!auth.signed_out, "policy changes are not explicit sign-out");
    load_fixture_profile(&mut auth, Store::Environment, None);
    assert!(!auth.connected());

    assert!(auth.initialize_with(Store::File));
    // A pending load from the opted-in policy must not win after opting out.
    auth.reload_pending = false;
    let (tx, rx) = mpsc::sync_channel(1);
    auth.profile_incoming = Some(rx);
    assert!(auth.initialize_with(Store::Environment));
    assert!(tx.send(Ok(Auth::connected_fixture().profile)).is_ok());
    assert!(auth.poll_with(
        |_| panic!("no store access"),
        |_, _| panic!("old worker must drain")
    ));
    assert!(!auth.connected());
    // An environment credential is still allowed under the new policy.
    load_fixture_profile(
        &mut auth,
        Store::Environment,
        Auth::connected_fixture().profile,
    );
    assert!(auth.connected());
}

#[test]
fn policy_reload_drains_accepted_write_without_applying_its_token() {
    let mut auth = Auth::connected_fixture();
    auth.store = Store::File;
    auth.committing = true;
    deliver(
        &mut auth,
        Ok(Reply::Authenticated(Arc::new("late-fixture".into()))),
    );
    assert!(auth.initialize_with(Store::Environment));
    assert!(auth.poll_with(
        |_| panic!("write was already accepted"),
        |_, _| panic!("must drain the accepted write first"),
    ));
    assert!(!auth.committing);
    assert!(auth.reload_pending);
    assert!(!auth.connected());
    load_fixture_profile(&mut auth, Store::Environment, None);
    assert!(!auth.connected());
}

#[test]
fn auth_priority_and_storage_errors_never_fall_back_silently() {
    assert_eq!(
        resolve_token(Some(" gh ".into()), Some("github".into()), || panic!(
            "must not read Keychain"
        ))
        .unwrap()
        .expose_secret(),
        "gh"
    );
    assert_eq!(
        resolve_token(Some(" ".into()), Some("github".into()), || panic!(
            "must not read Keychain"
        ))
        .unwrap()
        .expose_secret(),
        "github"
    );
    assert_eq!(
        resolve_token(None, None, || Ok(Some("saved".into())))
            .unwrap()
            .expose_secret(),
        "saved"
    );
    assert!(
        resolve_token(None, None, || Ok(None))
            .unwrap_err()
            .to_string()
            .contains("authentication required")
    );
    assert_eq!(
        resolve_token(None, None, || Err(Error::Token))
            .unwrap_err()
            .to_string(),
        Error::Token.to_string()
    );
    assert!(resolve_token(Some("bad\nsecret".into()), None, || panic!()).is_err());
    assert!(resolve_token(None, None, || Ok(Some("  ".into()))).is_err());
    assert_eq!(
        resolve_token(Some(" \t".into()), Some("\n".into()), || Ok(Some(
            " saved ".into()
        )))
        .unwrap()
        .expose_secret(),
        "saved"
    );
    assert!(
        resolve_token(
            Some("bad\nsecret".into()),
            Some("valid".into()),
            || panic!()
        )
        .is_err()
    );
}
