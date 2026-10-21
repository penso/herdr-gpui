//! A restart shows what the last run found before reading again: the
//! database is the app's own, agent-launcher's is carried over once, the
//! checks are kept beside the items, and the repository a checkout held is
//! cached until git says otherwise.
use super::service::{repository, start, wait};
use super::*;
use crate::cache::{Cache, Kind, path_in};
use crate::orchestrator::{
    location::{database_path_in, legacy_path_in},
    review::{Check, CheckState, PrStatus, Review},
    service::cache_key,
};
use herdr_client::ConnectTarget;
use std::collections::HashMap;

fn status() -> PrStatus {
    PrStatus {
        review: Some(Review::Approved),
        requested: vec!["octocat".into()],
        checks: vec![Check {
            name: "ci".into(),
            state: CheckState::Passed,
        }],
    }
}

#[test]
fn checks_are_kept_per_repository_and_replaced_whole() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("db").join("state.sqlite3");
    let mut store = Store::open(&path).unwrap();
    store
        .replace_pr_statuses("g/app", &HashMap::from([(1, status()), (2, status())]))
        .unwrap();
    store
        .replace_pr_statuses("g/other", &HashMap::from([(1, PrStatus::default())]))
        .unwrap();
    store
        .replace_pr_statuses("g/app", &HashMap::from([(2, status())]))
        .unwrap();
    drop(store);
    let statuses = Store::open(&path).unwrap().pr_statuses().unwrap();
    assert_eq!(
        statuses,
        HashMap::from([
            (("g/app".to_owned(), 2), status()),
            (("g/other".to_owned(), 1), PrStatus::default()),
        ])
    );
}

#[test]
fn agent_launchers_database_is_carried_over_once() {
    let dir = tempfile::tempdir().unwrap();
    let checkout = repository(dir.path());
    // As detection names it: canonical, without Windows' `\\?\` prefix.
    let git_dir = crate::orchestrator::repo::plain(
        std::fs::canonicalize(std::path::Path::new(&checkout).join(".git")).unwrap(),
    );
    let repo = Repository::Local(git_dir);
    let data = dir.path().join("data");
    let legacy = legacy_path_in(&data, &repo);
    let mut old = Store::open(&legacy).unwrap();
    old.save_run(&run("r1", Owner::HerdrGpui), None).unwrap();
    old.replace_pr_statuses("g/app", &HashMap::from([(1, status())]))
        .unwrap();
    drop(old);

    let service = start(dir.path(), checkout.clone());
    let snapshot = wait(&service, |snapshot| !snapshot.runs.is_empty());
    assert_eq!(snapshot.runs[0].id, "r1");
    assert_eq!(
        snapshot.pr_status.get(&("g/app".to_owned(), 1)),
        Some(&status())
    );
    drop(service);
    let path = database_path_in(&data, &repo);
    assert!(path.is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "{mode:o}");
    }

    // The copy is the app's now: a run agent-launcher adds later stays there.
    let mut old = Store::open(&legacy).unwrap();
    old.save_run(&run("r2", Owner::HerdrGpui), None).unwrap();
    drop(old);
    let service = start(dir.path(), checkout);
    let snapshot = wait(&service, |snapshot| snapshot.repo.is_some());
    let ids: Vec<_> = snapshot.runs.iter().map(|run| run.id.as_str()).collect();
    assert_eq!(ids, ["r1"]);
}

#[test]
fn the_repository_a_checkout_held_is_cached_until_git_disagrees() {
    let dir = tempfile::tempdir().unwrap();
    let checkout = repository(dir.path());
    let service = start(dir.path(), checkout.clone());
    // Installed agents are looked up once the repository is cached.
    let found = wait(&service, |snapshot| {
        snapshot.installed.is_some() || snapshot.installed_error.is_some()
    })
    .repo
    .unwrap();
    drop(service);
    let cache = Cache::open_at(&path_in(&dir.path().join("data"))).unwrap();
    let key = cache_key(&ConnectTarget::Local, &checkout);
    let entry = cache
        .get::<crate::orchestrator::repo::RepoInfo>(Kind::Repository, &key)
        .unwrap();
    assert_eq!(entry.map(|entry| entry.value), Some(found));

    // The folder is no repository anymore: what was cached goes with it.
    std::fs::remove_dir_all(std::path::Path::new(&checkout).join(".git")).unwrap();
    let service = start(dir.path(), checkout);
    let snapshot = wait(&service, |snapshot| snapshot.error.is_some());
    assert!(matches!(
        snapshot.error.as_deref(),
        Some(Error::NotARepository)
    ));
    assert!(snapshot.repo.is_none());
    assert!(snapshot.items.is_empty());
    assert_eq!(
        cache
            .get::<crate::orchestrator::repo::RepoInfo>(Kind::Repository, &key)
            .unwrap(),
        None
    );
}

/// Two windows opening one repository at once each copy agent-launcher's
/// database; the first copy in place stays, whole, and nothing else is
/// left in the folder.
#[test]
fn imports_at_once_leave_one_whole_copy() {
    let dir = tempfile::tempdir().unwrap();
    let legacy = dir.path().join("legacy").join("state.sqlite3");
    let mut old = Store::open(&legacy).unwrap();
    old.save_run(&run("r1", Owner::HerdrGpui), None).unwrap();
    drop(old);
    let path = dir.path().join("app").join("state.sqlite3");
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| Store::import(&legacy, &path).unwrap());
        }
    });
    let runs = Store::open(&path).unwrap().runs().unwrap();
    assert_eq!(runs.len(), 1);
    let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| !name.ends_with("-wal") && !name.ends_with("-shm"))
        .collect();
    assert_eq!(names, ["state.sqlite3"]);

    // A database already in place is never replaced.
    let mut store = Store::open(&path).unwrap();
    store.delete_run("r1").unwrap();
    drop(store);
    Store::import(&legacy, &path).unwrap();
    assert!(Store::open(&path).unwrap().runs().unwrap().is_empty());
}
