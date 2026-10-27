//! A fork checkout lists the `upstream` repository's pull requests and fetches
//! each head from the remote that actually holds it.

use super::*;
use crate::pull_request::Remote;

fn upstream() -> Origin {
    Origin {
        owner: "base-owner".into(),
        repo: "project".into(),
        remote: Remote::Upstream,
        fork_owner: Some("contributor".into()),
    }
}

fn listed() -> Vec<super::super::Item> {
    let response = json!({"data": {"repository": {
        "pullRequests": {"nodes": [
            {"number": 7, "title": "Maintainer branch", "isDraft": false,
             "headRefName": "release-fix", "author": {"login": "maintainer"},
             "headRepositoryOwner": {"login": "Base-Owner"}},
            {"number": 8, "title": "My own change", "isDraft": false,
             "headRefName": "feature", "author": {"login": "contributor"},
             "headRepositoryOwner": {"login": "CONTRIBUTOR"}},
            {"number": 9, "title": "Someone else's fork", "isDraft": false,
             "headRefName": "patch-1", "author": {"login": "outsider"},
             "headRepositoryOwner": {"login": "outsider"}}
        ]},
        "issues": {"nodes": []}
    }}});
    parse(&response, &upstream(), Kind::PullRequest).unwrap()
}

#[test]
fn upstream_listings_name_the_remote_each_head_lives_on() {
    let prs = listed();
    let rows: Vec<_> = prs
        .iter()
        .map(|pr| (pr.number, pr.fork_owner.as_deref(), pr.remote, pr.branch()))
        .collect();
    assert_eq!(
        rows,
        [
            (7, None, Remote::Upstream, "release-fix".to_owned()),
            (8, None, Remote::Origin, "feature".to_owned()),
            (9, Some("outsider"), Remote::Upstream, "pr/9".to_owned()),
        ]
    );
    assert_eq!(prs[0].base_ref(), "refs/remotes/upstream/release-fix");
    assert_eq!(prs[1].base_ref(), "refs/remotes/origin/feature");
    assert_eq!(prs[2].base_ref(), "refs/herdr/pull/9/head");
    assert_eq!(
        prs[2].url, "https://github.com/base-owner/project/pull/9",
        "rows link to the listed repository"
    );
}

#[test]
fn upstream_pull_requests_are_fetched_from_upstream_not_origin() {
    let temporary = tempfile::tempdir().unwrap();
    let config = temporary.path().join("gitconfig");
    std::fs::write(&config, "").unwrap();
    let base = temporary.path().join("base");
    let fork = temporary.path().join("fork");
    let checkout = temporary.path().join("checkout");
    let git = |path: &std::path::Path, args: &[&str]| {
        let output = std::process::Command::new("git")
            .env("GIT_CONFIG_GLOBAL", &config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_owned()
    };
    let commit = |path: &std::path::Path, message: &str| {
        git(
            path,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "--allow-empty",
                "-m",
                message,
            ],
        );
        git(path, &["rev-parse", "HEAD"])
    };
    std::fs::create_dir(&base).unwrap();
    git(&base, &["init", "-b", "main"]);
    commit(&base, "base");
    git(
        temporary.path(),
        &["clone", base.to_str().unwrap(), fork.to_str().unwrap()],
    );
    git(
        temporary.path(),
        &["clone", fork.to_str().unwrap(), checkout.to_str().unwrap()],
    );
    git(
        &checkout,
        &["remote", "add", "upstream", base.to_str().unwrap()],
    );
    // The same PR number on the fork must never be what is fetched.
    let decoy = commit(&fork, "fork decoy");
    git(&fork, &["update-ref", "refs/pull/9/head", &decoy]);
    let outsider = commit(&base, "outsider head");
    git(&base, &["update-ref", "refs/pull/9/head", &outsider]);
    git(&base, &["branch", "release-fix"]);
    let input = super::super::Target {
        host: crate::pull_request::Origin::Local,
        input: crate::pull_request::Input {
            checkout: Some(checkout.to_str().unwrap().into()),
            repo_key: Some(checkout.join(".git").to_str().unwrap().into()),
            branch: "main".into(),
        },
    };
    let prs = listed();
    super::super::fetch::fetch_branch(&input, &prs[2], &|| false).unwrap();
    assert_eq!(git(&checkout, &["rev-parse", &prs[2].base_ref()]), outsider);
    super::super::fetch::fetch_branch(&input, &prs[0], &|| false).unwrap();
    assert_eq!(git(&checkout, &["rev-parse", &prs[0].base_ref()]), outsider);
}
