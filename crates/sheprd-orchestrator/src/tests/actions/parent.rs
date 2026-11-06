use super::*;
use crate::actions::parent_of;
use sheprd_remote::teleport::{Envelope, HostSnapshot, SnapshotResult};

fn decoded(workspaces: &[(&str, bool)]) -> HostSnapshot {
    serde_json::from_str::<Envelope<SnapshotResult>>(&snapshot(workspaces))
        .unwrap()
        .result
        .snapshot
}

#[test]
fn a_linked_worktree_creates_through_its_repositorys_parent() {
    let open = decoded(&[("w1", true), ("w2", false), ("w3", true)]);
    assert_eq!(parent_of(&open, "w1"), Some("w2"));
    assert_eq!(parent_of(&open, "w2"), Some("w2"));
    assert_eq!(parent_of(&decoded(&[("w1", true)]), "w1"), None);
    // Herdr reports a workspace closed meanwhile.
    assert_eq!(parent_of(&open, "gone"), Some("gone"));
}

#[test]
fn another_repositorys_parent_is_never_used() {
    let mut snapshot = decoded(&[("other", false), ("w1", true), ("w2", false)]);
    if let Some(tree) = snapshot.workspaces[0].worktree.as_mut() {
        tree.repo_key = "/elsewhere/.git".into();
    }
    assert_eq!(parent_of(&snapshot, "w1"), Some("w2"));
    snapshot.workspaces.truncate(2);
    assert_eq!(parent_of(&snapshot, "w1"), None);
}

#[test]
fn dispatch_from_a_linked_worktree_uses_the_open_parent() {
    let dir = tempfile::tempdir().unwrap();
    let site = site(dir.path(), "");
    std::fs::write(
        dir.path().join("home/snapshot"),
        snapshot(&[("w1", true), ("w2", false)]),
    )
    .unwrap();
    perform(
        &site,
        &Action::Dispatch(Box::new(request())),
        &AtomicBool::new(false),
    )
    .unwrap();
    let calls = calls(dir.path());
    assert!(
        calls[1].starts_with("worktree create --workspace w2 --branch 377-icons "),
        "{calls:?}"
    );
}

#[test]
fn dispatch_without_an_open_parent_opens_the_main_checkout() {
    let dir = tempfile::tempdir().unwrap();
    let site = site(dir.path(), "");
    std::fs::write(dir.path().join("home/snapshot"), snapshot(&[("w1", true)])).unwrap();
    perform(
        &site,
        &Action::Dispatch(Box::new(request())),
        &AtomicBool::new(false),
    )
    .unwrap();
    let calls = calls(dir.path());
    assert_eq!(
        calls[1],
        format!("workspace create --cwd {} --no-focus", site.main_root)
    );
    assert!(
        calls[2].starts_with("worktree create --workspace w0 --branch 377-icons "),
        "{calls:?}"
    );
}
