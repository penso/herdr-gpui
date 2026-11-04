//! The cache keeps the last read per sign-in, so a restart lists it at
//! once, and the run's own read replaces it.
use super::*;
use crate::cache::{Cache, path_in};
use crate::security::saved::{restore, save};
use std::time::{Instant, SystemTime};

fn names(repos: &[Repo]) -> Vec<&str> {
    repos.iter().map(|repo| repo.name.as_str()).collect()
}

#[test]
fn a_read_is_kept_per_window_and_a_failed_half_keeps_its_last() {
    let dir = tempfile::tempdir().unwrap();
    let cache = Cache::open_at(&path_in(dir.path())).unwrap();
    // The key names the sign-ins and the window's checkouts (`saved::key`).
    let window = "me\n\nLocal:/src/one/.git";
    assert!(restore(&cache, window).is_none());
    let first = vec![
        Repo {
            reports: Ok(Reports::Open {
                reports: vec![report("GHSA-aaaa-bbbb-cccc", None, "2026-10-05T00:00:00Z")],
                more: false,
            }),
            ..repo("a/one", Ok(open(vec![alert(1, Severity::High)])))
        },
        repo("a/gone", Ok(Alerts::Off)),
    ];
    save(&cache, window, &first);
    let (repos, _) = restore(&cache, window).unwrap();
    assert_eq!(names(&repos), ["a/one", "a/gone"]);
    assert_eq!(repos[0].open(), first[0].open());
    assert_eq!(repos[0].reported(), first[0].reported());
    // Nothing that reads GitHub or starts an agent is kept.
    assert!(
        repos
            .iter()
            .all(|repo| repo.account.is_none() && repo.checkouts.is_empty())
    );

    // a/one's alerts fail this time but its reports are read anew; a/new's
    // reports fail on its first read; a/gone is no longer read.
    let second = vec![
        Repo {
            reports: Ok(Reports::Off),
            ..repo("a/one", Err(Error::Output))
        },
        Repo {
            reports: Err(Arc::new(Error::Output)),
            ..repo("a/new", Ok(open(vec![alert(3, Severity::Low)])))
        },
    ];
    save(&cache, window, &second);
    let (repos, _) = restore(&cache, window).unwrap();
    assert_eq!(names(&repos), ["a/one", "a/new"]);
    assert_eq!(repos[0].open(), first[0].open());
    assert_eq!(repos[0].reports.as_ref().ok(), Some(&Reports::Off));
    assert_eq!(repos[1].open(), second[1].open());
    assert!(matches!(&repos[1].reports, Err(error) if matches!(**error, Error::Unread)));

    // Another window, or another sign-in, reads none of it.
    assert!(restore(&cache, "me\n\nLocal:/src/two/.git").is_none());
    assert!(restore(&cache, "someone\n\nLocal:/src/one/.git").is_none());
}

#[test]
fn the_runs_own_read_wins_over_the_cache() {
    let mut security = Security::default();
    let at = SystemTime::UNIX_EPOCH;
    security.restore(vec![repo("a/cached", Ok(Alerts::Off))], at);
    assert_eq!(names(security.repos()), ["a/cached"]);
    assert_eq!(security.read_at(), Some(at));
    // Restoring changes nothing about when the run reads.
    assert!(security.due(Instant::now()));
    security.read_for_test(vec![repo("a/read", Ok(Alerts::Off))]);
    // A restore that lands late is dropped.
    security.restore(vec![repo("a/cached", Ok(Alerts::Off))], at);
    assert_eq!(names(security.repos()), ["a/read"]);
}
