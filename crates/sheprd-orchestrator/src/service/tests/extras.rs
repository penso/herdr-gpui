//! What opening a project read beside its repository is kept: the agents
//! the host has, and the branches, which the sync that follows reads again
//! only after that.
use super::*;

#[test]
fn branches_read_on_opening_are_not_read_again_by_the_next_sync() {
    let dir = tempfile::tempdir().unwrap();
    let (_service, mut worker, host) = opened(dir.path());
    let info = worker.snapshot.repo.clone().unwrap();
    let branch = "agent/377-icons-0123abcd".to_owned();
    let extras = Extras {
        programs: Some(vec![AgentKind::ALL[0].binary().to_owned()]),
        branches: Some(vec![branch.clone()]),
    };
    worker.found(&host, Ok((info, extras))).unwrap();
    assert_eq!(
        worker.snapshot.installed.as_deref(),
        Some(&vec![AgentKind::ALL[0]])
    );
    assert_eq!(*worker.snapshot.branches, std::slice::from_ref(&branch));
    // The checkout has no such branch: a sync that read again would drop it.
    worker.sync(None);
    assert_eq!(*worker.snapshot.branches, [branch]);
    worker.sync(None);
    assert!(worker.snapshot.branches.is_empty(), "later syncs read them");
}
