use super::super::items::{ItemInfo, describe, git_common_dir, list, select};
use crate::{
    cli::control::Items,
    orchestrator::{Item, ItemKey, Provider, PullRequest, SourceKey},
};
use chrono::{TimeZone, Utc};
use std::process::Command;

fn item(native_id: &str, pull_request: bool, updated: i64) -> Item {
    let mut item = Item::stand_in(ItemKey {
        source: SourceKey {
            provider: Provider::Github,
            host: "github.com".into(),
            repository: "/penso/app".into(),
        },
        native_id: native_id.into(),
    });
    item.title = format!("Title {native_id}");
    item.state = "open".into();
    item.updated_at = Utc.timestamp_opt(updated, 0).single();
    item.pull_request = pull_request.then(|| PullRequest {
        number: 12,
        additions: None,
        deletions: None,
        base_ref: "main".into(),
        head_ref: "fix".into(),
        base_sha: "a".into(),
        head_sha: "b".into(),
        head_repository: None,
    });
    item
}

#[test]
fn items_are_filtered_by_kind_newest_first() {
    let items = || {
        vec![
            item("1", false, 10),
            item("pr/12", true, 30),
            item("2", false, 20),
        ]
    };
    let identifiers = |infos: Vec<ItemInfo>| {
        infos
            .into_iter()
            .map(|info| info.identifier)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        identifiers(select(items(), Items::All)),
        ["#12", "#2", "#1"]
    );
    assert_eq!(identifiers(select(items(), Items::Issues)), ["#2", "#1"]);
    let prs = select(items(), Items::PullRequests);
    assert_eq!(prs.len(), 1);
    assert_eq!(prs[0].pull_request, Some(12));
    assert_eq!(describe(&prs), "#12\topen\tTitle pr/12");
}

fn git(dir: &std::path::Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

#[test]
fn a_repository_is_found_by_its_common_directory() {
    let folder = tempfile::tempdir().unwrap();
    let repo = folder.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    std::fs::create_dir(repo.join("src")).unwrap();
    let common = git_common_dir(&repo.join("src")).unwrap();
    assert_eq!(common, repo.join(".git").canonicalize().unwrap());
    let outside = folder.path().join("plain");
    std::fs::create_dir(&outside).unwrap();
    assert!(matches!(
        git_common_dir(&outside),
        Err(crate::Error::NotAGitRepository { .. })
    ));
}

#[test]
fn listing_never_creates_a_database() {
    let folder = tempfile::tempdir().unwrap();
    git(folder.path(), &["init", "-q"]);
    let common = git_common_dir(folder.path()).unwrap();
    let Some(path) =
        crate::orchestrator::database_path(&crate::orchestrator::Repository::Local(common))
    else {
        return;
    };
    // A fresh repository has nothing synced: the answer says so.
    let output = list(Some(folder.path()), Items::All).unwrap();
    assert_eq!(output.status, crate::control::client::exit::NOTHING);
    assert!(!path.exists());
}
