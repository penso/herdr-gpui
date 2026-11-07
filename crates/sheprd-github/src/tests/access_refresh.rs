use super::*;
use crate::{Account, installation::Access};

fn waiting(now: Instant) -> Auth {
    let mut auth = Auth::connected_fixture();
    auth.account = Account::additional("0123456789abcdef0123456789abcdef").unwrap();
    auth.store = Store::Keyring;
    auth.last_session_check = Some(now - Duration::from_secs(300));
    auth.next_session_check = Some(now + Duration::from_secs(300));
    auth.profile.as_mut().unwrap().access = Access::NotInstalled;
    auth
}

fn start_refresh(now: Instant) -> (Auth, mpsc::SyncSender<()>) {
    let mut auth = waiting(now);
    let (started_tx, started_rx) = mpsc::sync_channel(1);
    let (release_tx, release_rx) = mpsc::sync_channel(1);
    auth.recheck_access(now);
    assert_eq!(auth.next_session_check, Some(now));
    assert!(auth.poll_at(
        now,
        |_| panic!("no credential write"),
        move |token, store| {
            assert!(
                token.is_none(),
                "resolve the store instead of reusing a stale token"
            );
            assert_eq!(store, Store::Keyring);
            started_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let mut profile = Profile::fixture(
                "fixture-user",
                Arc::new("ghu_refreshed".into()),
                &["private-org"],
            );
            profile.access = Access::Ready;
            Ok(Some(profile))
        },
    ));
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    (auth, release_tx)
}

fn complete_refresh(auth: &mut Auth, release: mpsc::SyncSender<()>, now: Instant) {
    release.send(()).unwrap();
    let result = auth
        .profile_incoming
        .take()
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    auth.complete_profile_fixture(result);
    assert!(auth.poll_at(
        now,
        |_| panic!("drain refresh before any credential write"),
        |_, _| panic!("drain refresh before starting another"),
    ));
}

#[test]
fn access_rechecks_coalesce_and_never_overlap_an_inflight_refresh() {
    let now = Instant::now();
    let (mut auth, release) = start_refresh(now);
    for elapsed in [0, 10, 30, 60] {
        let at = now + Duration::from_secs(elapsed);
        auth.recheck_access(at);
        assert!(!auth.poll_at(
            at,
            |_| panic!("no write"),
            |_, _| panic!("no overlapping refresh")
        ));
    }
    complete_refresh(&mut auth, release, now + Duration::from_secs(60));
    let profile = auth.profile.as_ref().unwrap();
    assert_eq!(profile.access, Access::Ready);
    assert_eq!(
        profile.organizations.as_deref(),
        Some(&["private-org".to_owned()][..])
    );
    let next = auth.next_session_check;
    auth.recheck_access(now + Duration::from_secs(61));
    assert_eq!(
        auth.next_session_check, next,
        "ready accounts keep the normal interval"
    );
}

#[test]
fn a_recent_check_is_throttled_and_a_missing_schedule_is_created() {
    let now = Instant::now();
    let mut auth = waiting(now);
    auth.last_session_check = Some(now);
    auth.next_session_check = None;
    auth.recheck_access(now);
    assert_eq!(auth.next_session_check, Some(now + Duration::from_secs(30)));
    auth.recheck_access(now + Duration::from_secs(10));
    assert_eq!(auth.next_session_check, Some(now + Duration::from_secs(30)));
    auth.profile.as_mut().unwrap().access = Access::Unknown;
    auth.next_session_check = None;
    auth.recheck_access(now);
    assert!(auth.next_session_check.is_none());
}

#[test]
fn signing_out_fences_an_access_refresh_and_its_organization_routes() {
    let now = Instant::now();
    let (mut auth, release) = start_refresh(now);
    auth.sign_out();
    auth.recheck_access(now);
    assert!(auth.next_session_check.is_none());
    assert!(!auth.poll_at(
        now,
        |_| panic!("wait for refresh before deleting"),
        |_, _| panic!("signed out")
    ));
    complete_refresh(&mut auth, release, now);
    assert!(auth.profile.is_none());
    assert!(auth.signout_pending);
    assert!(auth.next_session_check.is_none());
}

#[test]
fn a_policy_change_fences_refresh_results_before_loading_the_replacement_account() {
    let now = Instant::now();
    let (mut auth, release) = start_refresh(now);
    assert!(auth.initialize_with(Store::Environment));
    auth.recheck_access(now);
    assert!(auth.next_session_check.is_none());
    complete_refresh(&mut auth, release, now);
    assert!(auth.profile.is_none());
    assert!(auth.reload_pending);
    assert!(auth.poll_at(
        now,
        |_| panic!("no write"),
        |token, store| {
            assert!(token.is_none());
            assert_eq!(store, Store::Environment);
            Ok(Some(Profile::fixture(
                "replacement",
                Arc::new("ghp_replacement".into()),
                &[],
            )))
        }
    ));
    let result = auth
        .profile_incoming
        .take()
        .unwrap()
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    auth.complete_profile_fixture(result);
    auth.poll_at(
        now,
        |_| panic!("no write"),
        |_, _| panic!("no duplicate load"),
    );
    let profile = auth.profile.as_ref().unwrap();
    assert_eq!(profile.login, "replacement");
    assert_eq!(profile.organizations, Some(Vec::new()));
    assert_eq!(profile.access, Access::Unknown);
}
