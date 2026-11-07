//! A remote glab comes to know after the repository was found, through an
//! `ssh_host` added later, is read as GitLab from the next sync on, without
//! reopening the view.
use super::*;

#[test]
fn a_mapping_added_after_opening_makes_the_remote_a_gitlab_source() {
    let dir = tempfile::tempdir().unwrap();
    let (_service, mut worker, _host) = opened(dir.path());
    worker.recognize(Vec::new);
    let source = |worker: &Worker| worker.snapshot.sources[0].clone();
    assert_eq!(source(&worker).key.provider, Provider::Github);
    assert!(matches!(source(&worker).state, SyncState::Unsupported));

    let config = "hosts:\n    gitlab.acme.com:\n        ssh_host: github.acme.com\n";
    worker.recognize(|| gitlab::cli_hosts::parse(config));
    let status = source(&worker);
    let expected = SourceKey {
        provider: Provider::Gitlab,
        host: "github.acme.com".into(),
        repository: "group/app".into(),
    };
    assert_eq!(status.key, expected, "the host and path keep their key");
    assert!(matches!(status.state, SyncState::Syncing { .. }));
    let remote = worker.snapshot.repo.as_ref().unwrap().remote.as_ref();
    assert_eq!(remote.map(|remote| &remote.source), Some(&expected));

    // Once GitLab, the config is not read again.
    worker.recognize(|| unreachable!());
    assert_eq!(source(&worker).key, expected);
}
