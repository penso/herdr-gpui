#![allow(clippy::unwrap_used)]

use super::{Auth, Device, Note, Profile, Reply, SETUP_MESSAGE, Store, VERIFY_URL};
use super::{
    credentials,
    device::TokenResponse,
    http::{LIMIT, authorization, graphql, pr_cooldown, response},
    log::{header, kind, public_sso},
    store,
    store::{KEYCHAIN, credential_bytes, resolve_token},
    token::Credential,
};
use crate::{Error, Result};
use secrecy::{ExposeSecret, SecretString};
use serde_json::Value;
use std::{
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

#[test]
fn avatar_refresh_is_scoped_to_the_verified_profile() {
    let mut auth = Auth::connected_fixture();
    let (tx, rx) = mpsc::sync_channel(1);
    auth.profile.as_mut().unwrap().avatar_updates = Some(rx);
    let image = Arc::new(gpui_kit::Image::empty());
    tx.send(image.clone()).unwrap();
    assert!(auth.poll_with_store(|_| panic!("avatar must not access credentials")));
    assert!(Arc::ptr_eq(
        auth.profile.as_ref().unwrap().avatar.as_ref().unwrap(),
        &image
    ));
    let (tx, rx) = mpsc::sync_channel(1);
    auth.profile.as_mut().unwrap().avatar_updates = Some(rx);
    auth.sign_out();
    assert!(!auth.connected());
    assert!(tx.send(image.clone()).is_err());
    auth.profile = Auth::connected_fixture().profile;
    assert!(auth.profile.as_ref().unwrap().avatar.is_none());
    let (tx, rx) = mpsc::sync_channel(1);
    auth.profile.as_mut().unwrap().avatar_updates = Some(rx);
    auth.signed_out = false;
    auth.initialized = false;
    auth.initialize(&crate::config::Config::default());
    assert!(!auth.connected());
    assert!(tx.send(image).is_err());
}

#[test]
fn copy_feedback_is_scoped_to_live_flow_and_expires() {
    let mut auth = Auth::fixture(true);
    assert!(!auth.copied());
    assert!(!auth.can_sign_out());
    assert_eq!(auth.copy_code(), Some("ABCD-1234"));
    assert!(auth.copied());
    auth.flow.as_mut().unwrap().copied_until = Some(Instant::now() - Duration::from_secs(1));
    assert!(auth.poll_with_store(|_| panic!("no storage for copy")));
    assert!(!auth.copied());
    auth.copy_code();
    auth.cancel();
    assert!(auth.copy_code().is_none());
    assert!(!auth.copied());
    auth = Auth::fixture(true);
    assert!(!auth.copied());
    auth.flow.as_mut().unwrap().deadline = Instant::now() - Duration::from_secs(1);
    assert!(auth.code().is_none());
    assert!(auth.copy_code().is_none());
    assert!(auth.poll_with_store(|_| panic!("expired code must not write")));
    assert!(auth.failed);
    assert!(!auth.busy());
    assert!(Auth::connected_fixture().can_sign_out());
}

#[test]
fn only_a_signed_release_build_uses_the_keychain() {
    // A signed release build keeps the Keychain whatever the config says.
    assert_eq!(Store::choose(false, true, false), Store::Keychain);
    assert_eq!(Store::choose(true, true, false), Store::Keychain);
    // An unsigned development build gets a new code identity on every rebuild,
    // so it uses the private file instead of re-prompting for Keychain access.
    assert_eq!(Store::choose(false, false, true), Store::File);
    assert_eq!(Store::choose(true, false, true), Store::File);
    // Everywhere else unencrypted storage stays an explicit opt-in.
    assert_eq!(Store::choose(false, false, false), Store::Environment);
    assert_eq!(Store::choose(true, false, false), Store::File);
    let mut config = crate::config::Config::default();
    #[cfg(target_os = "macos")]
    if !crate::RELEASE_BUILD {
        assert_eq!(Store::select(&config), Store::File);
    }
    config.github.allow_plaintext_credentials = true;
    assert_eq!(
        Store::select(&config),
        Store::choose(store::FILE, KEYCHAIN, false)
    );
    // Platforms without POSIX ownership and mode bits cannot keep the file
    // private, so opting in must not select it there.
    assert_eq!(store::FILE, cfg!(unix));
    if !store::FILE {
        assert_eq!(Store::select(&config), Store::Environment);
        assert!(matches!(
            credentials::store(std::path::Path::new("."), Some(&"token".into()), true),
            Err(Error::CredentialUnsupported)
        ));
        assert!(credentials::store(std::path::Path::new("."), None, false).is_ok());
    }
}
#[test]
fn credential_notes_state_where_tokens_are_kept() {
    assert!(Store::Environment.note(false).is_none());
    assert!(Store::Environment.note(true).is_none());
    assert!(matches!(Store::Keychain.note(false), Some(Note::Info(_))));
    assert!(
        Store::Keychain.note(true).is_none(),
        "a connected account already proved Keychain access"
    );
    for connected in [false, true] {
        let Some(Note::Warning(text)) = Store::File.note(connected) else {
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
fn live_session_renews_off_thread_without_restart_and_preserves_token_identity() {
    let mut auth = Auth::connected_fixture();
    auth.store = Store::File;
    let now = Instant::now();
    auth.next_session_check = Some(now + Duration::from_secs(1));
    assert!(!auth.poll_at(now, |_| panic!(), |_, _| panic!("not due")));
    let old = auth.profile.as_ref().unwrap().token.clone();
    assert!(auth.poll_at(
        now + Duration::from_secs(1),
        |_| panic!(),
        |token, store| {
            assert!(token.is_none(), "resolve the latest saved credential");
            assert_eq!(store, Store::File);
            assert_eq!(thread::current().name(), Some("herdr-github-profile"));
            let loaded =
                Credential::new("expired-access".into(), Some("refresh".into()), "client")?
                    .profile_with(
                        |token| {
                            if token.expose_secret() == "expired-access" {
                                return Err(Error::GitHubAuthentication);
                            }
                            let mut profile = Auth::connected_fixture().profile.unwrap();
                            profile.token = token;
                            Ok(profile)
                        },
                        |_| {
                            Credential::new(
                                "rotated-access".into(),
                                Some("rotated-refresh".into()),
                                "client",
                            )
                        },
                        |_| Ok(()),
                    )?;
            Ok(Some(loaded))
        },
    ));
    assert!(auth.connected(), "renewal must not flash the signed-out UI");
    let result = auth
        .profile_incoming
        .take()
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    let (tx, rx) = mpsc::sync_channel(1);
    tx.send(result).ok().unwrap();
    auth.profile_incoming = Some(rx);
    assert!(auth.poll_at(now, |_| panic!(), |_, _| panic!("one worker only")));
    let rotated = auth.profile.as_ref().unwrap().token.clone();
    assert!(!Arc::ptr_eq(&old, &rotated));
    assert_eq!(rotated.expose_secret(), "rotated-access");
    assert!(!auth.poll_at(now, |_| panic!(), |_, _| panic!("bounded check interval")));

    let mut same = Auth::connected_fixture().profile.unwrap();
    same.token = Arc::new("rotated-access".into());
    let (tx, rx) = mpsc::sync_channel(1);
    tx.send(Ok(Some(same))).ok().unwrap();
    auth.profile_incoming = Some(rx);
    auth.poll_at(now, |_| panic!(), |_, _| panic!());
    assert!(Arc::ptr_eq(&rotated, &auth.profile.as_ref().unwrap().token));
}

#[test]
fn live_session_transient_failures_preserve_account_and_retry_but_rejection_disconnects() {
    for error in [
        Error::GitHubStatus(503),
        Error::GitHubRateLimit,
        Error::GitHubForbidden,
        Error::CredentialPolicy,
    ] {
        let mut auth = Auth::connected_fixture();
        let now = Instant::now();
        let (tx, rx) = mpsc::sync_channel(1);
        tx.send(Err(error)).ok().unwrap();
        auth.profile_incoming = Some(rx);
        auth.poll_at(now, |_| panic!(), |_, _| panic!());
        assert!(auth.connected());
        assert!(auth.failed);
        assert!(auth.next_session_check.unwrap() > now);
        assert!(!auth.poll_at(now, |_| panic!(), |_, _| panic!("no tight retry")));
    }
    let mut auth = Auth::connected_fixture();
    let (tx, rx) = mpsc::sync_channel(1);
    tx.send(Err(Error::GitHubAuthentication)).ok().unwrap();
    auth.profile_incoming = Some(rx);
    auth.poll_at(Instant::now(), |_| panic!(), |_, _| panic!());
    assert!(!auth.connected());
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
    for store in [Store::Environment, Store::File, Store::Keychain] {
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
fn signout_discards_late_profile_and_auth_without_environment_reactivation() {
    let mut auth = Auth::connected_fixture();
    let (tx, rx) = mpsc::sync_channel(1);
    auth.profile_incoming = Some(rx);
    deliver(&mut auth, Ok(Reply::Token(credential("late-fixture"))));
    auth.sign_out();
    assert!(!auth.connected());
    assert!(!auth.loading_profile());
    assert!(auth.incoming.is_none());
    assert!(tx.send(Ok(Auth::connected_fixture().profile)).is_ok());
    auth.poll_with_store(|_| panic!("profile must drain before deletion"));
    assert!(auth.profile_incoming.is_none());
    assert!(!auth.connected());
    // Reload/reconnect cannot read environment or disk after explicit sign-out.
    auth.initialize(&crate::config::Config::default());
    assert!(!auth.loading_profile());
    auth.poll_with_store(|token| {
        assert!(token.is_none());
        Err(std::io::Error::other("mock removal failure").into())
    });
    let reply = auth
        .incoming
        .take()
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    deliver(&mut auth, reply);
    auth.poll_with_store(|_| panic!("no second store operation"));
    assert!(!auth.connected());
    assert!(auth.failed);
    assert_eq!(auth.message.as_deref(), Some("mock removal failure"));
}

#[test]
fn signout_serializes_after_accepted_write_and_discards_its_profile() {
    let mut auth = Auth::connected_fixture();
    auth.committing = true;
    deliver(
        &mut auth,
        Ok(Reply::Authenticated(Arc::new("late-fixture".into()))),
    );
    auth.sign_out();
    auth.cancel(); // Dismissal must not cancel credential removal.
    auth.poll_with_store(|_| panic!("write must complete before deletion"));
    assert!(!auth.connected());
    assert!(!auth.loading_profile());
    assert!(auth.signout_pending);
    auth.poll_with_store(|token| {
        assert!(token.is_none());
        Ok(())
    });
    let reply = auth
        .incoming
        .take()
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    deliver(&mut auth, reply);
    auth.poll_with_store(|_| panic!("already removed"));
    assert!(!auth.busy());
    assert!(!auth.connected());
    assert!(
        auth.message
            .as_deref()
            .unwrap()
            .contains("Environment tokens are suppressed")
    );
}

#[test]
fn signout_drains_inflight_profile_refresh_before_deleting_its_rotated_credential() {
    let saved = Arc::new(std::sync::Mutex::new(Some(
        Credential::new("old-access".into(), Some("old-refresh".into()), "client")
            .unwrap()
            .encode()
            .unwrap(),
    )));
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    let mut auth = Auth::connected_fixture();
    let worker_saved = saved.clone();
    auth.load_profile_with(None, move |token, _| {
        assert!(token.is_none());
        let credential = Credential::decode(worker_saved.lock().unwrap().as_ref().unwrap())?;
        credential
            .profile_with(
                |token| {
                    if token.expose_secret() == "old-access" {
                        return Err(Error::GitHubAuthentication);
                    }
                    assert_eq!(token.expose_secret(), "new-access");
                    let mut profile = Auth::connected_fixture().profile.unwrap();
                    profile.token = token;
                    Ok(profile)
                },
                |_| {
                    started_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    Credential::new("new-access".into(), Some("new-refresh".into()), "client")
                },
                |value| {
                    *worker_saved.lock().unwrap() = Some(value.expose_secret().into());
                    Ok(())
                },
            )
            .map(Some)
    });
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    auth.sign_out();
    assert!(!auth.poll_with(
        |_| panic!("refresh must finish before deletion"),
        |_, _| panic!("signed out"),
    ));
    assert!(auth.profile_incoming.is_some());
    assert!(auth.signout_pending);
    assert!(!auth.committing);
    assert!(auth.incoming.is_none());
    assert!(!auth.connected());
    assert!(!auth.loading_profile());

    release_tx.send(()).unwrap();
    let result = auth
        .profile_incoming
        .take()
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert_eq!(
        result
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .token
            .expose_secret(),
        "new-access"
    );
    let (tx, rx) = mpsc::sync_channel(1);
    tx.send(result).ok().unwrap();
    auth.profile_incoming = Some(rx);
    assert!(auth.poll_with(
        |_| panic!("drain the late result before deletion"),
        |_, _| panic!("signed out"),
    ));
    assert!(auth.profile_incoming.is_none());
    assert!(auth.signout_pending);
    assert!(!auth.connected());
    let worker_saved = saved.clone();
    assert!(auth.poll_with(
        move |token| {
            assert!(token.is_none());
            let value = worker_saved.lock().unwrap().take().unwrap();
            let credential = Credential::decode(&value)?;
            assert_eq!(credential.access_token.expose_secret(), "new-access");
            assert_eq!(
                credential.refresh_token.as_ref().unwrap().expose_secret(),
                "new-refresh"
            );
            Ok(())
        },
        |_, _| panic!("signed out"),
    ));
    let reply = auth
        .incoming
        .take()
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    deliver(&mut auth, reply);
    assert!(auth.poll_with(
        |_| panic!("already removed"),
        |_, _| panic!("late profile must not reactivate the session"),
    ));
    assert!(saved.lock().unwrap().is_none());
    assert!(auth.signed_out);
    assert!(!auth.connected());
    assert!(!auth.busy());
    assert!(!auth.failed);
}

#[test]
fn profile_loading_success_error_and_idle_do_not_start_device_auth() {
    let mut auth = Auth::default();
    for result in [
        Ok(Auth::connected_fixture().profile),
        Err(std::io::Error::other("mock profile failure").into()),
        Ok(None),
    ] {
        let (tx, rx) = mpsc::sync_channel(1);
        tx.send(result).ok().unwrap();
        auth.profile_incoming = Some(rx);
        assert!(auth.loading_profile());
        assert!(auth.poll_with_store(|_| panic!("profile does not persist tokens")));
        assert!(!auth.loading_profile());
        assert!(auth.incoming.is_none());
        assert!(auth.flow.is_none());
        assert_eq!(auth.failed, auth.message.is_some());
    }
    assert!(!auth.connected());
}

fn device() -> Device {
    serde_json::from_str::<Device>(r#"{"device_code":"fixture-device", "user_code":"ABCD-1234", "verification_uri":"https://github.com/login/device", "expires_in":900, "interval":5}"#).unwrap().validate().unwrap()
}
#[test]
fn setup_fixture_describes_public_config_and_environment_override() {
    let auth = Auth::fixture(false);
    assert_eq!(auth.message.as_deref(), Some(SETUP_MESSAGE));
    assert!(SETUP_MESSAGE.contains("[github] oauth_client_id"));
    assert!(SETUP_MESSAGE.contains("HERDR_GITHUB_OAUTH_CLIENT_ID"));
    assert!(SETUP_MESSAGE.contains("GitHub App or OAuth App public client ID"));
}
fn token_reply(value: Value) -> Result<Reply> {
    super::token_reply(serde_json::from_value(value).unwrap(), "fixture-client")
}
fn credential(token: &str) -> Credential {
    Credential::new(token.into(), None, "fixture-client").unwrap()
}
fn deliver(auth: &mut Auth, reply: Result<Reply>) {
    let (tx, rx) = mpsc::sync_channel(1);
    tx.send(reply).ok().unwrap();
    auth.incoming = Some(rx);
}
fn waiting() -> Auth {
    let mut auth = Auth::default();
    deliver(
        &mut auth,
        Ok(Reply::Device(
            device(),
            "fixture-client".into(),
            Instant::now(),
        )),
    );
    assert!(auth.poll());
    auth
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
        resolve_token(None, None, || Err(std::io::Error::other("locked").into()))
            .unwrap_err()
            .to_string(),
        "locked"
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
#[test]
fn credentials_and_authorization_debug_are_redacted() {
    let token = credential_bytes(b"fixture-access-secret".to_vec()).unwrap();
    assert!(!format!("{token:?}").contains("fixture-access-secret"));
    let header = authorization(&token).unwrap();
    assert!(header.is_sensitive());
    assert_eq!(header.to_str().unwrap(), "Bearer fixture-access-secret");
    assert!(!format!("{header:?}").contains("fixture-access-secret"));
    let device = device();
    let debug = format!("{device:?}");
    assert!(!debug.contains("fixture-device"));
    assert!(!debug.contains("ABCD-1234"));
    let error = credential_bytes(b"private-invalid-secret\xff".to_vec()).unwrap_err();
    assert!(matches!(error, Error::GitHubEncoding(_)));
    assert_eq!(error.to_string(), "Invalid GitHub credential encoding.");
    assert!(authorization(&SecretString::from("private\nsecret")).is_err());
}
#[test]
fn oauth_responses_deserialize_directly_to_redacted_secrets() {
    let reply = |body: &[u8]| {
        ureq::http::Response::builder()
            .status(200)
            .body(ureq::Body::builder().data(body.to_vec()))
            .unwrap()
    };
    let parsed: TokenResponse = response(
        "test",
        reply(br#"{"access_token":"fixture-access-secret","refresh_token":"fixture-refresh-secret","token_type":"bearer"}"#),
    )
    .unwrap();
    assert!(!format!("{parsed:?}").contains("fixture-access-secret"));
    assert!(!format!("{parsed:?}").contains("fixture-refresh-secret"));
    let Reply::Token(token) = super::token_reply(parsed, "fixture-client").unwrap() else {
        panic!()
    };
    assert_eq!(token.access_token.expose_secret(), "fixture-access-secret");
    assert_eq!(
        token.refresh_token.as_ref().unwrap().expose_secret(),
        "fixture-refresh-secret"
    );
    assert!(!format!("{token:?}").contains("fixture-access-secret"));
    assert!(!format!("{token:?}").contains("fixture-refresh-secret"));
    let mut auth = waiting();
    deliver(&mut auth, Ok(Reply::Token(token)));
    assert!(auth.poll_with_store(|value| {
        assert_eq!(thread::current().name(), Some("herdr-github-auth"));
        let value = value.unwrap();
        assert!(!format!("{value:?}").contains("fixture-access-secret"));
        assert!(!format!("{value:?}").contains("fixture-refresh-secret"));
        let record: Value = serde_json::from_str(value.expose_secret()).unwrap();
        assert_eq!(record["version"], 1);
        assert_eq!(record["client_id"], "fixture-client");
        let saved = Credential::decode(value)?;
        assert_eq!(saved.access_token.expose_secret(), "fixture-access-secret");
        assert_eq!(
            saved.refresh_token.as_ref().unwrap().expose_secret(),
            "fixture-refresh-secret"
        );
        Ok(())
    }));
    let persisted = auth
        .incoming
        .take()
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap()
        .unwrap();
    let Reply::Authenticated(token) = persisted else {
        panic!("the credential must be persisted before authentication completes");
    };
    assert_eq!(token.expose_secret(), "fixture-access-secret");
    for body in [
        br#"{"access_token":"private-secret","token_type":123}"#.as_slice(),
        br#"{"access_token":"private-secret","access_token":"duplicate"}"#,
        b"private-invalid-secret\xff",
    ] {
        assert_eq!(
            response::<TokenResponse>("test", reply(body))
                .unwrap_err()
                .to_string(),
            "Invalid GitHub JSON response."
        );
    }
    let parsed: Device = response("test", reply(br#"{"device_code":"fixture-device","user_code":"ABCD-1234","verification_uri":"https://github.com/login/device","expires_in":900}"#)).unwrap();
    assert_eq!(
        parsed.validate().unwrap().user_code.expose_secret(),
        "ABCD-1234"
    );
}
#[test]
fn expired_token_reply_is_not_persisted() {
    let mut auth = waiting();
    auth.flow.as_mut().unwrap().deadline = Instant::now();
    deliver(&mut auth, Ok(Reply::Token(credential("expired-secret"))));
    auth.poll_with_store(|_| panic!("expired token must never be stored"));
    assert!(!auth.busy());
    assert!(auth.code().is_none());
    assert!(auth.message.as_ref().unwrap().contains("expired"));
}
#[test]
fn pr_rate_limits_have_bounded_account_wide_cooldowns() {
    let now = std::time::UNIX_EPOCH + Duration::from_secs(1000);
    let mut headers = ureq::http::HeaderMap::new();
    assert_eq!(pr_cooldown(200, &headers, now), None);
    for status in [401, 403, 429] {
        assert_eq!(
            pr_cooldown(status, &headers, now),
            Some(Duration::from_secs(3600))
        );
    }
    headers.insert("retry-after", "600".parse().unwrap());
    headers.insert("x-ratelimit-reset", "2200".parse().unwrap());
    assert_eq!(
        pr_cooldown(429, &headers, now),
        Some(Duration::from_secs(1200))
    );
    headers.insert("retry-after", "18446744073709551615".parse().unwrap());
    assert_eq!(
        pr_cooldown(429, &headers, now),
        Some(Duration::from_secs(86400))
    );
    headers.insert("retry-after", "invalid".parse().unwrap());
    headers.insert("x-ratelimit-reset", "0".parse().unwrap());
    assert_eq!(
        pr_cooldown(403, &headers, now),
        Some(Duration::from_secs(300))
    );
}

#[test]
fn bounded_http_parsing_and_safe_errors() {
    let reply = |status, body: Vec<u8>| {
        ureq::http::Response::builder()
            .status(status)
            .body(ureq::Body::builder().data(body))
            .unwrap()
    };
    for (status, message) in [
        (401, "authentication required"),
        (403, "denied access"),
        (429, "rate limit"),
        (302, "request failed"),
        (500, "request failed"),
    ] {
        let error =
            response::<Value>("test", reply(status, b"private-error-secret".to_vec())).unwrap_err();
        assert!(error.to_string().contains(message));
        assert!(!error.to_string().contains("private-error-secret"));
    }
    assert_eq!(
        response::<Value>("test", reply(200, b"{\"ok\":true}".to_vec())).unwrap()["ok"],
        true
    );
    assert!(response::<Value>("test", reply(200, b"not-json".to_vec())).is_err());
    assert!(response::<Value>("test", reply(200, vec![b' '; LIMIT as usize + 1])).is_err());
    assert!(
        graphql(
            &"fixture".into(),
            "",
            Value::Null,
            Duration::from_secs(1),
            || true,
            &mut None
        )
        .unwrap_err()
        .to_string()
        .contains("cancelled")
    );
}
#[test]
fn rejected_device_responses_name_the_failing_check() {
    let device = |key: &str, value: Value| {
        let mut v = serde_json::json!({"device_code":"fixture", "user_code":"ABCD-1234", "verification_uri":VERIFY_URL, "expires_in":900, "interval":5});
        v[key] = value;
        serde_json::from_value::<Device>(v).unwrap()
    };
    for (key, value, rejection) in [
        ("device_code", serde_json::json!(""), "device_code"),
        ("user_code", serde_json::json!("bad\ncode"), "user_code"),
        ("user_code", serde_json::json!(""), "user_code"),
        (
            "verification_uri",
            serde_json::json!("https://github.example.test/login/device"),
            "verification_uri",
        ),
        ("expires_in", serde_json::json!(901), "expires_in"),
        ("interval", serde_json::json!(0), "interval"),
    ] {
        let device = device(key, value);
        assert_eq!(device.rejection(), Some(rejection));
        assert!(device.validate().is_err());
    }
    assert_eq!(
        device("interval", serde_json::json!(5)).rejection(),
        None,
        "a valid response must not report a rejection"
    );
}

#[test]
fn diagnostics_keep_public_details_and_drop_the_sso_request_id() {
    assert_eq!(
        public_sso("required; url=https://github.com/orgs/acme/sso?authorization_request=SECRET"),
        "required; url=https://github.com/orgs/acme/sso"
    );
    assert_eq!(public_sso(""), "");
    let mut headers = ureq::http::HeaderMap::new();
    assert_eq!(header(&headers, "x-github-request-id"), "");
    headers.insert("x-github-request-id", "ABCD:1234".parse().unwrap());
    assert_eq!(header(&headers, "x-github-request-id"), "ABCD:1234");
    // Categories stay stable so a log filter keeps working across releases.
    assert_eq!(kind(&Error::GitHubForbidden), "forbidden");
    assert_eq!(kind(&Error::GitHubStatus(500)), "status");
    assert_eq!(kind(&Error::GitHubWorker("profile")), "worker");
    assert_eq!(kind(&Error::PrTimeout), "other");
}

#[test]
fn device_validation_and_oauth_error_lifecycle() {
    for (key, value) in [
        ("verification_uri", serde_json::json!("https://evil.test")),
        ("expires_in", serde_json::json!(901)),
        ("interval", serde_json::json!(0)),
        ("user_code", serde_json::json!("bad\ncode")),
    ] {
        let mut v = serde_json::json!({"device_code":"fixture", "user_code":"ABCD-1234", "verification_uri":VERIFY_URL, "expires_in":900, "interval":5});
        v[key] = value;
        assert!(
            serde_json::from_value::<Device>(v)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    assert!(matches!(
        token_reply(serde_json::json!({"error":"authorization_pending"})),
        Ok(Reply::Pending(false))
    ));
    assert!(matches!(
        token_reply(serde_json::json!({"error":"slow_down"})),
        Ok(Reply::Pending(true))
    ));
    for error in [
        "expired_token",
        "access_denied",
        "incorrect_client_credentials",
        "unknown",
    ] {
        assert!(
            token_reply(serde_json::json!({"error":error, "error_description":"private-secret"}))
                .err()
                .unwrap()
                .to_string()
                .find("private-secret")
                .is_none()
        );
    }
    assert!(
        token_reply(serde_json::json!({"access_token":"fixture", "token_type":"mac"})).is_err()
    );
    assert!(matches!(
        token_reply(serde_json::json!({"access_token":"fixture", "token_type":"bearer"})),
        Ok(Reply::Token(_))
    ));
}
#[test]
fn pending_slowdown_expiry_and_cancel_do_not_store_stale_tokens() {
    let mut auth = waiting();
    assert_eq!(auth.code(), Some("ABCD-1234"));
    deliver(&mut auth, Ok(Reply::Pending(true)));
    auth.poll();
    assert_eq!(auth.flow.as_ref().unwrap().interval, 10);
    deliver(&mut auth, Ok(Reply::Pending(false)));
    auth.poll();
    assert_eq!(auth.flow.as_ref().unwrap().interval, 10);
    auth.flow.as_mut().unwrap().deadline = Instant::now();
    auth.poll();
    assert!(!auth.busy());
    assert!(auth.message.as_ref().unwrap().contains("expired"));
    for reply in [
        Reply::Token(credential("fixture")),
        Reply::Device(device(), "client".into(), Instant::now()),
    ] {
        let mut auth = waiting();
        deliver(&mut auth, Ok(reply));
        auth.cancel();
        auth.poll_with_store(|_| panic!("cancelled token must never be stored"));
        assert!(!auth.busy());
        assert!(auth.code().is_none());
    }
}
#[test]
fn accepted_token_uses_store_off_thread_and_reports_failure() {
    let mut auth = waiting();
    deliver(&mut auth, Ok(Reply::Token(credential("fixture-token"))));
    auth.poll_with_store(|token| {
        assert_eq!(
            token.map(ExposeSecret::expose_secret),
            Some("fixture-token")
        );
        assert_eq!(thread::current().name(), Some("herdr-github-auth"));
        Err(std::io::Error::other("mock Keychain locked").into())
    });
    auth.cancel(); // Accepted commits cannot be cancelled halfway through Keychain I/O.
    let reply = auth
        .incoming
        .take()
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    deliver(&mut auth, reply);
    auth.poll();
    assert_eq!(auth.message.as_deref(), Some("mock Keychain locked"));
    assert!(!auth.busy());
}
