use super::*;
use crate::installation::{Access, App, Installation, Listing, assess, check, list};

fn listing(installations: Value) -> Listing {
    serde_json::from_value(serde_json::json!({ "installations": installations })).unwrap()
}

fn app(permissions: Value) -> App {
    serde_json::from_value(serde_json::json!({ "permissions": permissions })).unwrap()
}

fn installed(id: u64, login: &str, kind: &str, permissions: Value) -> Value {
    serde_json::json!({
        "id": id,
        "app_slug": "herdr-ui",
        "account": { "login": login, "type": kind },
        "permissions": permissions,
    })
}

const WANTED: &str = r#"{"checks":"read","contents":"write","issues":"read","metadata":"read","pull_requests":"write","statuses":"read"}"#;

fn wanted() -> Value {
    serde_json::from_str(WANTED).unwrap()
}

#[test]
fn installations_with_every_permission_are_ready() {
    let mut more = wanted();
    more["contents"] = "admin".into();
    let listing = listing(serde_json::json!([
        installed(1, "me", "User", wanted()),
        installed(2, "acme", "Organization", more),
    ]));
    assert_eq!(assess(&listing, &app(wanted())), Access::Ready);
}

#[test]
fn a_missing_or_lower_permission_waits_for_its_owner() {
    let old = serde_json::json!({"issues":"read","metadata":"read","pull_requests":"read"});
    let listing = listing(serde_json::json!([
        installed(7, "zeta", "Organization", old.clone()),
        installed(3, "me", "User", old),
        installed(9, "acme", "Organization", wanted()),
    ]));
    // The user's own account comes first, then organizations by login.
    assert_eq!(
        assess(&listing, &app(wanted())),
        Access::Pending(vec![
            Installation::fixture("me", false, 3),
            Installation::fixture("zeta", true, 7),
        ])
    );
}

#[test]
fn organization_permissions_never_wait_on_a_personal_account() {
    let mut asked = wanted();
    asked["members"] = "read".into();
    asked["organization_projects"] = "write".into();
    let listing = listing(serde_json::json!([installed(1, "me", "User", wanted())]));
    assert_eq!(assess(&listing, &app(asked.clone())), Access::Ready);
    let listing = listing_of_org(wanted());
    assert!(matches!(assess(&listing, &app(asked)), Access::Pending(_)));
}

fn listing_of_org(permissions: Value) -> Listing {
    listing(serde_json::json!([installed(
        1,
        "acme",
        "Organization",
        permissions
    )]))
}

#[test]
fn an_unknown_level_is_never_taken_as_approved() {
    let mut asked = wanted();
    asked["checks"] = "superpower".into();
    assert!(matches!(
        assess(&listing_of_org(wanted()), &app(asked)),
        Access::Pending(_)
    ));
    // One the installation holds at a level this build does not know is
    // not held against it.
    let mut held = wanted();
    held["checks"] = "superpower".into();
    assert_eq!(assess(&listing_of_org(held), &app(wanted())), Access::Ready);
}

#[test]
fn an_owner_with_an_unexpected_login_is_not_linked() {
    let listing = listing(serde_json::json!([installed(
        1,
        "../evil",
        "Organization",
        serde_json::json!({})
    )]));
    assert_eq!(assess(&listing, &app(wanted())), Access::Ready);
}

#[test]
fn approval_links_point_at_each_owners_settings() {
    assert_eq!(
        Installation::fixture("me", false, 42).url(),
        "https://github.com/settings/installations/42"
    );
    assert_eq!(
        Installation::fixture("acme", true, 43).url(),
        "https://github.com/organizations/acme/settings/installations/43"
    );
}

#[test]
fn only_a_github_app_user_token_is_checked() {
    // These return before any request, so no network is needed.
    for token in ["ghp_classic", "gho_oauth", "github_pat_fine", "plain"] {
        let token = SecretString::from(token);
        let listing = list(&token).unwrap();
        assert!(listing.is_none());
        assert_eq!(check(&token, listing.as_ref()).unwrap(), Access::Unknown);
    }
}

#[test]
fn a_failed_check_keeps_what_the_last_one_found() {
    let token = Arc::new(SecretString::from("ghu_token"));
    let mut old = Profile::fixture("me", token.clone(), &[]);
    old.access = Access::NotInstalled;
    let mut new = Profile::fixture("me", token.clone(), &[]);
    new.keep_access(&old);
    assert_eq!(new.access, Access::NotInstalled);
    // A check that answered wins, and another login inherits nothing.
    let mut ready = Profile::fixture("me", token.clone(), &[]);
    ready.access = Access::Ready;
    ready.keep_access(&old);
    assert_eq!(ready.access, Access::Ready);
    let mut other = Profile::fixture("someone", token, &[]);
    other.keep_access(&old);
    assert_eq!(other.access, Access::Unknown);
    // A token that replaced the App's is never checked, so the App's old
    // warning does not follow it.
    let mut pat = Profile::fixture("me", Arc::new(SecretString::from("ghp_classic")), &[]);
    pat.keep_access(&old);
    assert_eq!(pat.access, Access::Unknown);
}

#[test]
fn a_waiting_account_is_checked_again_soon_but_not_constantly() {
    let now = Instant::now();
    let mut auth = Auth::connected_fixture();
    auth.last_session_check = Some(now);
    auth.next_session_check = Some(now + Duration::from_secs(300));
    auth.recheck_access(now);
    assert_eq!(
        auth.next_session_check,
        Some(now + Duration::from_secs(300))
    );

    auth.profile.as_mut().unwrap().access = Access::NotInstalled;
    auth.recheck_access(now);
    let soon = now + Duration::from_secs(30);
    assert_eq!(auth.next_session_check, Some(soon));
    // Looking again keeps the check already due rather than pushing it back.
    auth.recheck_access(now + Duration::from_secs(10));
    assert_eq!(auth.next_session_check, Some(soon));
}
