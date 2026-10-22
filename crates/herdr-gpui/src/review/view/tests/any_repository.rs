//! A space opened in a repository Herdr did not make as a worktree carries
//! no daemon worktree metadata, only its branch and directory. Its review
//! is found from that directory, as its pull request already is; a folder
//! outside any repository still has nothing to review.
use super::{HerdrWindow, State, window};
use crate::{browser::Location, pull_request::Input};
use gpui::{Entity, VisualTestContext};
use herdr_client::protocol::ClientShellWorktree;
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

fn git(directory: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
        ])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .env_remove("GIT_DIR")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
}

/// A repository cloned as a user would, with an edit waiting in it. Its
/// top level and the subdirectory the space was opened in.
fn clone(root: &Path) -> (PathBuf, PathBuf) {
    let upstream = root.join("upstream");
    std::fs::create_dir_all(upstream.join("src")).unwrap();
    git(&upstream, &["init", "-q", "-b", "main"]);
    std::fs::write(upstream.join("src/lib.rs"), "fn a() {}\n").unwrap();
    git(&upstream, &["add", "-A"]);
    git(&upstream, &["commit", "-qm", "base"]);
    git(root, &["clone", "-q", upstream.to_str().unwrap(), "clone"]);
    let clone = root.join("clone");
    std::fs::write(clone.join("src/lib.rs"), "fn a() { todo!() }\n").unwrap();
    let space = clone.join("src");
    (clone, space)
}

/// Shows workspace `w0` in `directory` on the window's own local daemon, as
/// the daemon reports it, and lets the Git chip follow it.
fn space_in(
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
    directory: &Path,
    branch: Option<&str>,
    worktree: Option<ClientShellWorktree>,
) {
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            let mut shown = (**view.live.snapshot.as_ref().unwrap()).clone();
            let space = shown
                .workspaces
                .iter_mut()
                .find(|workspace| workspace.workspace_id == "w0")
                .unwrap();
            space.new_workspace_cwd = directory.to_str().unwrap().into();
            space.branch = branch.map(Into::into);
            space.worktree = worktree;
            view.live.snapshot = Some(Arc::new(shown));
            view.live.status = crate::state::ConnectionStatus::Connected;
            view.live.local_daemon_peer = true;
            view.update_git();
        })
    });
}

/// Opens the review as the menu's Review row does and waits for its read:
/// the checkout its tab is saved by and the files it lists.
fn review(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> (Input, Vec<String>) {
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_review(window, cx)));
    cx.run_until_parked();
    let checkout = cx.update(|_, cx| {
        cx.try_global::<crate::browser::Store>().and_then(|store| {
            store.tabs().find_map(|tab| match &tab.location {
                Some(Location::Review { checkout }) => Some(Input::from(checkout)),
                _ => None,
            })
        })
    });
    let Some(checkout) = checkout else {
        panic!("no review tab opened");
    };
    let files = view.read_with(cx, |view, _| {
        let review = view.reviews.values().next().unwrap();
        match &review.state {
            State::Loaded(loaded) => loaded
                .diff
                .files
                .iter()
                .map(|file| file.path.clone())
                .collect(),
            State::Failed(error) => panic!("the review failed: {error}"),
            State::Loading => panic!("the review is still loading"),
        }
    });
    (checkout, files)
}

#[gpui::test]
fn a_space_in_a_cloned_repository_is_reviewed_from_its_directory(cx: &mut gpui::TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (_, space) = clone(root.path());
    let (view, cx) = window(cx, None);
    space_in(&view, cx, &space, Some("main"), None);
    let (checkout, files) = review(&view, cx);
    assert_eq!(
        checkout,
        Input {
            checkout: Some(space.to_str().unwrap().into()),
            repo_key: None,
            branch: "main".into(),
        },
        "saved by the space's directory, which the worker resolves"
    );
    // Read from the repository's top level, not the subdirectory.
    assert_eq!(files, ["src/lib.rs"]);
}

#[gpui::test]
fn a_worktree_herdr_made_is_still_reviewed_by_its_daemon_key(cx: &mut gpui::TestAppContext) {
    let root = tempfile::tempdir().unwrap();
    let (clone, space) = clone(root.path());
    let key = clone.join(".git").to_str().unwrap().to_owned();
    let (view, cx) = window(cx, None);
    let worktree = ClientShellWorktree {
        key: key.clone(),
        label: "clone".into(),
        is_linked_worktree: false,
    };
    space_in(&view, cx, &space, Some("main"), Some(worktree));
    let (checkout, files) = review(&view, cx);
    assert_eq!(
        checkout,
        Input {
            checkout: None,
            repo_key: Some(key),
            branch: "main".into(),
        },
        "daemon metadata wins, and the directory is never trusted"
    );
    assert_eq!(files, ["src/lib.rs"]);
}

#[gpui::test]
fn a_folder_outside_any_repository_has_nothing_to_review(cx: &mut gpui::TestAppContext) {
    let folder = tempfile::tempdir().unwrap();
    let (view, cx) = window(cx, None);
    // The daemon reports no branch for a folder Git does not know.
    space_in(&view, cx, folder.path(), None, None);
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_review(window, cx)));
    view.read_with(cx, |view, _| {
        assert!(view.git.tracked().is_none());
        assert!(view.reviews.is_empty());
        let flash = view.flash.as_ref().map(|(flash, _)| flash.text.to_string());
        assert_eq!(flash.as_deref(), Some("No local checkout to review"));
    });
}
