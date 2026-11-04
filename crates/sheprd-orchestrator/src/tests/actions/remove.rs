use super::*;

/// A dispatched run whose session is reread from the database, with its
/// workspace's recorded checkout set to `path`.
fn run_pointing_at(site: &Site, path: Option<PathBuf>) -> String {
    let mut store = Store::open(&site.database).unwrap();
    let mut run = store.runs().unwrap().remove(0);
    let session = store.sessions().unwrap().remove(0);
    run.workspace.as_mut().unwrap().path = path;
    let id = run.id.clone();
    store.update_run(&run, Some(&session)).unwrap();
    id
}

/// The host's snapshot no longer lists the run's workspace, as when the
/// run's pane has exited.
fn workspace_gone(dir: &Path) {
    std::fs::write(dir.join("home/snapshot"), snapshot(&[("w1", false)])).unwrap();
}

#[test]
fn remove_goes_through_herdr_while_the_workspace_lives() {
    let dir = tempfile::tempdir().unwrap();
    let site = site(dir.path(), "");
    let cancelled = AtomicBool::new(false);
    perform(&site, &Action::Dispatch(Box::new(request())), &cancelled).unwrap();
    let id = Store::open(&site.database)
        .unwrap()
        .runs()
        .unwrap()
        .remove(0)
        .id;

    perform(&site, &Action::Remove { run: id }, &cancelled).unwrap();

    let calls = calls(dir.path());
    assert_eq!(
        &calls[calls.len() - 2..],
        ["api snapshot", "worktree remove --workspace w9 --force"]
    );
    assert!(
        Store::open(&site.database)
            .unwrap()
            .runs()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn remove_falls_back_to_git_once_the_workspace_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let site = site(dir.path(), "");
    let cancelled = AtomicBool::new(false);
    perform(&site, &Action::Dispatch(Box::new(request())), &cancelled).unwrap();
    // The checkout dispatch recorded does not exist; point the run at a real
    // linked worktree of the test repository.
    let checkout = dir.path().join("wt");
    assert!(
        Command::new("git")
            .args([
                "-C",
                &site.main_root,
                "worktree",
                "add",
                "-q",
                "-b",
                "repro"
            ])
            .arg(&checkout)
            .status()
            .unwrap()
            .success()
    );
    let id = run_pointing_at(&site, Some(checkout.clone()));
    workspace_gone(dir.path());

    perform(&site, &Action::Remove { run: id }, &cancelled).unwrap();

    let calls = calls(dir.path());
    assert_eq!(calls.last().unwrap(), "api snapshot");
    assert!(
        !calls.iter().any(|call| call.starts_with("worktree remove")),
        "{calls:?}"
    );
    assert!(!checkout.exists(), "the worktree was not removed");
    let listed = Command::new("git")
        .args(["-C", &site.main_root, "worktree", "list", "--porcelain"])
        .output()
        .unwrap();
    // Match the listed path, not any "wt": the random temporary directory
    // name every path shares can contain it too.
    let listed = String::from_utf8_lossy(&listed.stdout);
    assert!(
        !listed
            .lines()
            .any(|line| line.starts_with("worktree ") && line.ends_with("/wt")),
        "git still lists the worktree: {listed}"
    );
    assert!(
        Store::open(&site.database)
            .unwrap()
            .runs()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn remove_without_a_recorded_checkout_only_forgets_the_run() {
    let dir = tempfile::tempdir().unwrap();
    let site = site(dir.path(), "");
    let cancelled = AtomicBool::new(false);
    perform(&site, &Action::Dispatch(Box::new(request())), &cancelled).unwrap();
    let id = run_pointing_at(&site, None);
    workspace_gone(dir.path());

    perform(&site, &Action::Remove { run: id }, &cancelled).unwrap();

    let calls = calls(dir.path());
    assert_eq!(calls.last().unwrap(), "api snapshot");
    assert!(
        Store::open(&site.database)
            .unwrap()
            .runs()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn remove_when_the_checkout_is_already_gone_only_forgets_the_run() {
    let dir = tempfile::tempdir().unwrap();
    let site = site(dir.path(), "");
    let cancelled = AtomicBool::new(false);
    perform(&site, &Action::Dispatch(Box::new(request())), &cancelled).unwrap();
    let id = run_pointing_at(&site, Some(dir.path().join("missing")));
    workspace_gone(dir.path());

    perform(&site, &Action::Remove { run: id }, &cancelled).unwrap();

    assert!(
        Store::open(&site.database)
            .unwrap()
            .runs()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn remove_reports_a_checkout_git_cannot_remove() {
    let dir = tempfile::tempdir().unwrap();
    let site = site(dir.path(), "");
    let cancelled = AtomicBool::new(false);
    perform(&site, &Action::Dispatch(Box::new(request())), &cancelled).unwrap();
    // A directory git does not know, so the fallback fails instead of
    // silently forgetting a checkout that is still there.
    std::fs::create_dir_all(dir.path().join("leftover")).unwrap();
    let id = run_pointing_at(&site, Some(dir.path().join("leftover")));
    workspace_gone(dir.path());

    let error = perform(&site, &Action::Remove { run: id }, &cancelled).unwrap_err();
    assert!(matches!(
        error,
        Error::Script {
            operation: "Removing the worktree",
            ..
        }
    ));
    assert!(
        dir.path().join("leftover").exists(),
        "a failed remove left the checkout alone"
    );
    assert_eq!(
        Store::open(&site.database).unwrap().runs().unwrap().len(),
        1,
        "a failed remove keeps the run"
    );
}
