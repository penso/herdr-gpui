use super::*;
use crate::pull_request::fetch::Head;

struct Directory(std::path::PathBuf);

impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn fixture_repo(branch: &str) -> (Directory, impl Fn(&[&str])) {
    let id = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let name = format!("herdr-fork-test-{}-{suffix}-{id}", std::process::id());
    let directory = Directory(std::env::temp_dir().join(name));
    std::fs::create_dir(&directory.0).unwrap();
    let dir = directory.0.clone();
    let git = move |args: &[&str]| {
        let mut command = Command::new("git");
        command.arg("-C").arg(&dir).args(args);
        let (ok, text) = run(&mut command, Instant::now() + TIMEOUT, &|| false).unwrap();
        assert!(ok, "fixture git failed: {text}");
    };
    git(&["init", "--quiet", "--template=", "-b", branch]);
    (directory, git)
}

#[test]
fn base_repository_prefers_upstream_over_origin_and_falls_back_when_missing() {
    let (dir, git) = fixture_repo("main");
    let path = dir.0.to_str().unwrap();
    let base = || base_repository(path, Instant::now() + TIMEOUT, &|| false).unwrap();

    git(&[
        "config",
        "--local",
        "remote.origin.url",
        "https://github.com/contributor/project.git",
    ]);
    assert_eq!(base(), origin_base("contributor", "project"));

    git(&[
        "config",
        "--local",
        "remote.upstream.url",
        "https://github.com/base-owner/project.git",
    ]);
    assert_eq!(
        base(),
        Base {
            owner: "base-owner".into(),
            repo: "project".into(),
            remote: Remote::Upstream,
            fork_owner: Some("contributor".into()),
        }
    );
    // Picker and PR creation resolve the same base as PR lookup.
    let input = Input {
        checkout: Some(path.into()),
        repo_key: Some(dir.0.join(".git").to_str().unwrap().into()),
        branch: "main".into(),
    };
    assert_eq!(
        local_repository(&input, Instant::now() + TIMEOUT, &|| false).unwrap(),
        base()
    );
    // Pushing still goes to the fork.
    assert_eq!(
        origin_repository(path, Instant::now() + TIMEOUT, &|| false).unwrap(),
        ("contributor".into(), "project".into())
    );

    git(&[
        "config",
        "--local",
        "remote.origin.url",
        "https://unsupported.invalid/contributor/project.git",
    ]);
    assert_eq!(base().fork_owner, None);

    git(&[
        "config",
        "--local",
        "remote.origin.url",
        "https://github.com/contributor/project.git",
    ]);
    git(&[
        "config",
        "--local",
        "remote.upstream.url",
        "https://unsupported.invalid/base-owner/project.git",
    ]);
    assert_eq!(base(), origin_base("contributor", "project"));
}

#[test]
fn an_upstream_naming_origins_own_repository_is_not_a_fork() {
    // Some clones add `upstream` for the same repository; creating a pull
    // request there must stay the ordinary same-repository path.
    let (dir, git) = fixture_repo("main");
    let path = dir.0.to_str().unwrap();
    git(&[
        "config",
        "--local",
        "remote.origin.url",
        "https://github.com/base-owner/project.git",
    ]);
    git(&[
        "config",
        "--local",
        "remote.upstream.url",
        "git@github.com:Base-Owner/Project.git",
    ]);
    let base = base_repository(path, Instant::now() + TIMEOUT, &|| false).unwrap();
    assert_eq!(base, origin_base("base-owner", "project"));
    assert_eq!(base.head("feature").unwrap(), "feature");
}

#[test]
fn a_new_pull_request_head_is_qualified_only_across_a_fork() {
    let own = origin_base("contributor", "project");
    assert_eq!(own.head("feature").unwrap(), "feature");
    assert!(own.is_own_base("main", "main"));
    assert!(!own.is_own_base("feature", "main"));

    let fork = Base {
        owner: "base-owner".into(),
        repo: "project".into(),
        remote: Remote::Upstream,
        fork_owner: Some("contributor".into()),
    };
    assert_eq!(fork.head("feature").unwrap(), "contributor:feature");
    // The fork's main is a different branch from upstream's main.
    assert!(!fork.is_own_base("main", "main"));

    let unknown_fork = Base {
        fork_owner: None,
        ..fork
    };
    assert!(matches!(
        unknown_fork.head("feature"),
        Err(Error::PullRequest(crate::pull_request::Error::Origin))
    ));
}

#[test]
fn target_detects_fork_setup_and_tracks_or_defaults_head() {
    let (dir, git) = fixture_repo("feature");
    let path = dir.0.to_str().unwrap();
    git(&[
        "config",
        "--local",
        "remote.origin.url",
        "git@github.com:contributor/project.git",
    ]);
    git(&[
        "config",
        "--local",
        "remote.upstream.url",
        "git@github.com:base-owner/project.git",
    ]);

    let input = Input {
        checkout: Some(path.into()),
        repo_key: Some(dir.0.join(".git").to_str().unwrap().into()),
        branch: "feature".into(),
    };

    let mut origins = super::super::fetch::Origins::default();
    let found = target(
        &input,
        &Origin::Local,
        &mut origins,
        Instant::now() + TIMEOUT,
        &|| false,
    )
    .unwrap();
    assert_eq!(found.owner, "base-owner");
    assert_eq!(found.repo, "project");
    match found.ask {
        Ask::Branch { branch, head } => {
            assert_eq!(branch, "feature");
            assert_eq!(
                head,
                Some(Head {
                    owner: "contributor".into(),
                    repo: "project".into(),
                    branch: "feature".into(),
                })
            );
        }
        Ask::Number(_) => panic!("expected branch ask"),
    }

    // Explicit tracking with renamed branch on fork
    git(&["config", "--local", "branch.feature.remote", "origin"]);
    git(&[
        "config",
        "--local",
        "branch.feature.merge",
        "refs/heads/renamed-feat",
    ]);

    let found = target(
        &input,
        &Origin::Local,
        &mut origins,
        Instant::now() + TIMEOUT,
        &|| false,
    )
    .unwrap();
    assert_eq!(found.owner, "base-owner");
    assert_eq!(found.repo, "project");
    match found.ask {
        Ask::Branch { branch, head } => {
            assert_eq!(branch, "renamed-feat");
            assert_eq!(
                head,
                Some(Head {
                    owner: "contributor".into(),
                    repo: "project".into(),
                    branch: "renamed-feat".into(),
                })
            );
        }
        Ask::Number(_) => panic!("expected branch ask"),
    }
}

#[test]
fn target_on_fork_preserves_numbered_fork_branch() {
    let (dir, git) = fixture_repo("pr/42");
    let path = dir.0.to_str().unwrap();
    git(&[
        "config",
        "--local",
        "remote.origin.url",
        "git@github.com:contributor/project.git",
    ]);
    git(&[
        "config",
        "--local",
        "remote.upstream.url",
        "git@github.com:base-owner/project.git",
    ]);

    let input = Input {
        checkout: Some(path.into()),
        repo_key: Some(dir.0.join(".git").to_str().unwrap().into()),
        branch: "pr/42".into(),
    };

    let mut origins = super::super::fetch::Origins::default();
    let found = target(
        &input,
        &Origin::Local,
        &mut origins,
        Instant::now() + TIMEOUT,
        &|| false,
    )
    .unwrap();
    assert_eq!(found.owner, "base-owner");
    assert_eq!(found.repo, "project");
    assert!(matches!(found.ask, Ask::Number(42)));
}

#[test]
fn target_on_non_fork_repo_leaves_untracked_head_none() {
    let (dir, git) = fixture_repo("feature");
    let path = dir.0.to_str().unwrap();
    git(&[
        "config",
        "--local",
        "remote.origin.url",
        "git@github.com:base-owner/project.git",
    ]);

    let input = Input {
        checkout: Some(path.into()),
        repo_key: Some(dir.0.join(".git").to_str().unwrap().into()),
        branch: "feature".into(),
    };

    let mut origins = super::super::fetch::Origins::default();
    let found = target(
        &input,
        &Origin::Local,
        &mut origins,
        Instant::now() + TIMEOUT,
        &|| false,
    )
    .unwrap();
    assert_eq!(found.owner, "base-owner");
    assert_eq!(found.repo, "project");
    match found.ask {
        Ask::Branch { branch, head } => {
            assert_eq!(branch, "feature");
            assert_eq!(head, None);
        }
        Ask::Number(_) => panic!("expected branch ask"),
    }
}

#[test]
fn fork_pull_request_matches_against_upstream_base_with_fork_head() {
    let head = Head {
        owner: "contributor".into(),
        repo: "project".into(),
        branch: "feature".into(),
    };
    let mut pr = response()[0].clone();
    pr["url"] = "https://github.com/base-owner/project/pull/8".into();
    pr["headRefName"] = "feature".into();
    pr["headRepositoryOwner"] = serde_json::json!({"login": "contributor"});
    pr["headRepository"] = serde_json::json!({"name": "project"});
    pr["isCrossRepository"] = true.into();

    let graphql = serde_json::json!({
        "data": {
            "repository": {
                "pullRequests": {
                    "nodes": [pr]
                }
            }
        }
    });

    let parsed = parse_graphql(graphql, "base-owner", "project", "feature", Some(&head))
        .unwrap()
        .unwrap();

    assert_eq!(parsed.number, 8);
    assert_eq!(parsed.url, "https://github.com/base-owner/project/pull/8");
    assert_eq!(parsed.head_repository_owner.login, "contributor");
}
