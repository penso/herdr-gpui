//! Which account reads a repository, and what a change of sign-in drops.
use super::*;
use crate::github::{Profile, Tokens};
use crate::security::fetch::{FULLY_READ, Found, readable};
use secrecy::ExposeSecret;

fn tokens(login: &str, token: &str) -> Tokens {
    Tokens::new(&Profile::fixture(login, Arc::new(token.into()), &[]), [])
}

#[test]
fn a_read_counts_its_alerts_and_its_reporting_as_readable() {
    let mut full = repo("a/one", Ok(Alerts::Off));
    assert_eq!(readable(&full), FULLY_READ);
    full.reports = Ok(Reports::Hidden);
    assert_eq!(readable(&full), 1);
    let refused = Repo {
        reports: Err(Arc::new(Error::Output)),
        ..repo("a/one", Err(Error::Output))
    };
    assert_eq!(readable(&refused), 0);
    assert_eq!(readable(&reported("a/one", Vec::new())), FULLY_READ);
}

#[test]
fn another_hosts_account_is_kept_to_try_but_the_same_token_once() {
    let (first, again, other) = (
        tokens("penso", "first"),
        tokens("penso", "first"),
        tokens("work", "second"),
    );
    let mut found = Found {
        name: "a/one".into(),
        accounts: vec![("Local", &first)],
    };
    found.add(("laptop", &again));
    found.add(("server", &other));
    let hosts: Vec<&str> = found.accounts.iter().map(|(host, _)| *host).collect();
    assert_eq!(hosts, ["Local", "server"]);
    assert_eq!(found.accounts[1].1.for_owner("a").expose_secret(), "second");
}

#[gpui::test]
fn signing_out_drops_what_the_last_sign_in_read(cx: &mut gpui::TestAppContext) {
    let (view, cx) = section::window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.menu.github.profile =
                Some(Profile::fixture("penso", Arc::new("token".into()), &[]));
            view.poll_security(cx);
            view.security.install(
                vec![reported(
                    "a/one",
                    vec![report("GHSA-aaaa-aaaa-aaaa", None, "2026-10-05T00:00:00Z")],
                )],
                Instant::now(),
                SystemTime::now(),
            );
            // A read in flight for the old account must not land afterwards.
            view.security.task = Some(gpui::Task::ready(()));
            view.menu.github.profile = None;
            view.poll_security(cx);
            assert!(view.security.repos().is_empty());
            assert!(view.security.read_at().is_none());
            assert!(!view.security.busy());
        });
    });
}

#[gpui::test]
fn another_account_drops_the_old_read_too(cx: &mut gpui::TestAppContext) {
    let (view, cx) = section::window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.menu.github.profile =
                Some(Profile::fixture("penso", Arc::new("token".into()), &[]));
            view.poll_security(cx);
            view.security.install(
                vec![reported("a/one", Vec::new())],
                Instant::now(),
                SystemTime::now(),
            );
            view.poll_security(cx);
            assert_eq!(view.security.repos().len(), 1, "the same sign-in keeps it");
            view.menu.github.profile =
                Some(Profile::fixture("someone", Arc::new("other".into()), &[]));
            view.poll_security(cx);
            assert!(view.security.repos().is_empty());
        });
    });
}
