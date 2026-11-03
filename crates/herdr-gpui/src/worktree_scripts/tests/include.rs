use super::*;
use std::path::Path;

fn git(dir: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
}

/// A setup after creation, of `linked` in the repository whose main checkout
/// is `main`.
fn created(view: &HerdrWindow, main: &Path, linked: &Path) -> Launch {
    Launch {
        checkout: Some(Checkout {
            path: linked.to_str().unwrap().into(),
            root: Some(main.to_str().unwrap().into()),
        }),
        requested: false,
        ..launch(view, ScriptKind::Setup, None)
    }
}

#[gpui::test]
fn a_setup_reads_the_scripts_file_worktreeinclude_copied_in(cx: &mut gpui::TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (main, linked) = (dir.path().join("main"), dir.path().join("linked"));
    std::fs::create_dir(&main).unwrap();
    git(&main, &["init", "-q", "-b", "main"]);
    std::fs::write(main.join(".gitignore"), ".herdr/\n.worktreeinclude\n").unwrap();
    git(&main, &["add", "."]);
    git(&main, &["commit", "-qm", "init"]);
    git(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "linked",
            linked.to_str().unwrap(),
        ],
    );
    write_scripts(&main, "[scripts]\nsetup = \"npm ci\"\n");
    std::fs::write(main.join(".worktreeinclude"), ".herdr/worktree.toml\n").unwrap();
    let peer = MockPeer::advertising(&["worktree.list", "tab.create"]);
    let (view, cx) = cx.add_window_view(fixture_window);
    // Another kind of script copies nothing, so finds no file.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            connect(view, &peer, cx);
            let run = Launch {
                kind: ScriptKind::Run,
                ..created(view, &main, &linked)
            };
            view.start_worktree_script(run, cx).unwrap();
        })
    });
    cx.run_until_parked();
    assert!(!linked.join(config::PATH).exists());
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            assert_eq!(step(view), "none");
            view.start_worktree_script(created(view, &main, &linked), cx)
                .unwrap();
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            // The file was absent from the new checkout until the copy.
            assert_eq!(step(view), "asking");
            assert!(view.local_error.is_none());
        })
    });
    assert_eq!(
        std::fs::read_to_string(linked.join(config::PATH)).unwrap(),
        "[scripts]\nsetup = \"npm ci\"\n"
    );
}

#[gpui::test]
fn a_failed_copy_stops_the_setup_and_says_why(cx: &mut gpui::TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let (main, linked) = (dir.path().join("main"), dir.path().join("linked"));
    // A list in a directory Git does not know.
    std::fs::create_dir(&main).unwrap();
    std::fs::write(main.join(".worktreeinclude"), ".env\n").unwrap();
    write_scripts(&linked, "[scripts]\nsetup = \"npm ci\"\n");
    let peer = MockPeer::advertising(&["worktree.list", "tab.create"]);
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            connect(view, &peer, cx);
            view.start_worktree_script(created(view, &main, &linked), cx)
                .unwrap();
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            assert_eq!(step(view), "none");
            let error = view.local_error.as_deref().unwrap();
            assert!(
                error.starts_with(
                    "The setup script did not start: Could not copy the files .worktreeinclude names:"
                ),
                "{error}"
            );
        })
    });
}
