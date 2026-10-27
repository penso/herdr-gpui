use super::super::{context, remote};
use super::*;
use std::path::Path;

mod ssh_commands;

fn git(path: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
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
}

fn script(directory: &Path, script: &str) {
    let output = std::process::Command::new("/bin/sh")
        .current_dir(directory)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(["-c", script])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn remote_fetch_preserves_branches_and_notes_belong_to_the_created_worktree() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("upstream");
    let checkout = temp.path().join("host's checkout $(false)");
    std::fs::create_dir(&source).unwrap();
    git(&source, &["init", "-b", "main"]);
    let commit = |message| {
        git(
            &source,
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
        git(&source, &["rev-parse", "HEAD"])
    };
    let base = commit("base");
    git(
        temp.path(),
        &[
            "clone",
            source.to_str().unwrap(),
            checkout.to_str().unwrap(),
        ],
    );
    git(&checkout, &["branch", "pr/51"]);
    let head = commit("pull request");
    git(&source, &["update-ref", "refs/pull/51/head", &head]);
    let item = &parse(&response(), &origin(), Kind::PullRequest).unwrap()[1];
    script(
        temp.path(),
        &remote::fetch_script(checkout.join(".git").to_str().unwrap(), item).unwrap(),
    );
    assert_eq!(git(&checkout, &["rev-parse", &item.base_ref()]), head);
    assert_eq!(git(&checkout, &["rev-parse", "pr/51"]), base);
    assert_eq!(git(&checkout, &["rev-parse", "HEAD"]), base);

    let linked = temp.path().join("linked ' $(false)");
    git(
        &checkout,
        &["worktree", "add", linked.to_str().unwrap(), "pr/51"],
    );
    let mut item = item.clone();
    item.title = "Don't run $(touch INJECTED) or `touch INJECTED`\nsecond line".into();
    script(
        temp.path(),
        &context::remote_script(linked.to_str().unwrap(), &item, &origin()).unwrap(),
    );
    let directory = git(&linked, &["rev-parse", "--absolute-git-dir"]);
    let directory = Path::new(&directory).join("herdr");
    let note: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(directory.join("context.json")).unwrap())
            .unwrap();
    assert_eq!(note["title"], item.title);
    assert_eq!(note["number"], 51);
    assert!(
        std::fs::read_to_string(directory.join("CONTEXT.md"))
            .unwrap()
            .contains("Pull request: #51")
    );
    assert!(!checkout.join(".git/herdr").exists());
    assert!(!temp.path().join("INJECTED").exists());
    assert!(git(&linked, &["status", "--porcelain"]).is_empty());
}

#[test]
fn invalid_remote_paths_are_refused_before_building_a_command() {
    let item = &parse(&response(), &origin(), Kind::Issue).unwrap()[0];
    for path in ["relative", "~/repo", "/repo\nother", "C:\\repo"] {
        assert!(matches!(
            remote::fetch_script(path, item),
            Err(Error::Client(herdr_client::Error::InvalidGitDir))
        ));
        assert!(context::remote_script(path, item, &origin()).is_err());
    }
}
