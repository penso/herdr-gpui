use super::*;
use bonjour::{Permission, permitted};
use sheprd_client::LocalNetworkAccess;

/// What the form was told while the permission was checked.
fn run(
    first: LocalNetworkAccess,
    after_wait: LocalNetworkAccess,
    open_for: usize,
) -> (Permission, Vec<bool>, usize) {
    let mut reports = Vec::new();
    let mut checks = 0;
    let permission = permitted(
        || first,
        |keep_waiting| {
            // The wait asks before each check, as the real one does.
            while keep_waiting() {
                checks += 1;
                if checks == 3 {
                    return after_wait;
                }
            }
            LocalNetworkAccess::Denied
        },
        &mut |waiting| {
            reports.push(waiting);
            reports.len() <= open_for
        },
    );
    (permission, reports, checks)
}

#[test]
fn an_allowed_or_unknown_permission_browses_without_waiting() {
    for access in [LocalNetworkAccess::Allowed, LocalNetworkAccess::Unknown] {
        let (permission, reports, checks) = run(access, LocalNetworkAccess::Denied, usize::MAX);
        assert_eq!(permission, Permission::Granted);
        assert!(reports.is_empty());
        assert_eq!(checks, 0);
    }
}

/// macOS refuses while its alert is up; allowing it continues the search,
/// and the form hears that it is no longer waiting.
#[test]
fn a_refusal_waits_for_the_user_then_searches() {
    let (permission, reports, _) = run(
        LocalNetworkAccess::Denied,
        LocalNetworkAccess::Allowed,
        usize::MAX,
    );
    assert_eq!(permission, Permission::Granted);
    assert_eq!(reports.first(), Some(&true));
    assert_eq!(reports.last(), Some(&false));
}

#[test]
fn a_refusal_that_outlasts_the_wait_is_reported() {
    let (permission, reports, _) = run(
        LocalNetworkAccess::Denied,
        LocalNetworkAccess::Denied,
        usize::MAX,
    );
    assert_eq!(permission, Permission::Refused);
    assert!(reports.iter().all(|waiting| *waiting));
}

/// Closing the form stops the wait at its next check.
#[test]
fn a_closed_form_stops_the_wait() {
    let (permission, _, checks) = run(LocalNetworkAccess::Denied, LocalNetworkAccess::Allowed, 1);
    assert_eq!(permission, Permission::Cancelled);
    assert_eq!(checks, 0);
}

#[test]
fn a_refusal_names_the_setting_without_asking_for_a_restart() {
    let message = crate::Error::LocalNetworkDenied.to_string();
    assert!(message.contains("Privacy & Security › Local Network"));
    assert!(!message.contains("reopen"));
}
