#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use std::{fs, path::Path};

/// Git isolated from the user's configuration, so their global excludes do
/// not decide what counts as ignored here.
#[cfg(not(windows))]
const ENV: &str = "GIT_CONFIG_GLOBAL=/dev/null\nGIT_CONFIG_NOSYSTEM=1\nexport GIT_CONFIG_GLOBAL GIT_CONFIG_NOSYSTEM\n";

#[cfg(not(windows))]
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
        .expect("run git");
    assert!(output.status.success(), "git {args:?}: {output:?}");
}

/// A main checkout ignoring `ignored`, with a linked worktree beside it, in a
/// directory whose name a shell must quote.
#[cfg(not(windows))]
fn repository(ignored: &str) -> (tempfile::TempDir, std::path::PathBuf, std::path::PathBuf) {
    let dir = tempfile::Builder::new()
        .prefix("it's a repo ")
        .tempdir()
        .unwrap();
    let (main, linked) = (dir.path().join("main"), dir.path().join("linked"));
    fs::create_dir(&main).unwrap();
    git(&main, &["init", "-q", "-b", "main"]);
    fs::write(main.join(".gitignore"), ignored).unwrap();
    fs::write(main.join("README.md"), "committed\n").unwrap();
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
    (dir, main, linked)
}

/// Run the copy from `main` into `linked` with Git isolated.
#[cfg(not(windows))]
fn copy_isolated(main: &Path, linked: &Path) -> Vec<u8> {
    let mut output = Vec::new();
    run_script(
        ScriptHost::Local,
        &format!(
            "{ENV}{}",
            script(main.to_str().unwrap(), linked.to_str().unwrap())
        ),
        std::io::empty(),
        &mut output,
        ScriptLimits {
            output: MAX_OUTPUT,
            idle: IDLE,
        },
        &AtomicBool::new(false),
    )
    .unwrap();
    output
}

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[cfg(not(windows))]
#[test]
fn copies_only_untracked_ignored_files_the_list_names() {
    let (_dir, main, linked) = repository(".env*\n.herdr/\nCLAUDE.local.md\n");
    write(
        &main.join(PATH),
        ".env*\n!.env.local\n.herdr/worktree.toml\nCLAUDE.local.md\nnotes.txt\nREADME.md\nmissing.txt\n",
    );
    write(&main.join(".env"), "SECRET=1\n");
    write(&main.join(".env.local"), "excluded by the list\n");
    write(&main.join(".herdr/worktree.toml"), "[scripts]\n");
    write(&main.join(".herdr/other.toml"), "not named\n");
    write(&main.join("CLAUDE.local.md"), "main's\n");
    write(&main.join("notes.txt"), "untracked but not ignored\n");
    write(&main.join("README.md"), "edited in main\n");
    write(&linked.join("CLAUDE.local.md"), "the worktree's own\n");

    let printed = copy_isolated(&main, &linked);

    assert_eq!(printed, b"\n\n", "one line per copied file");
    assert_eq!(
        fs::read_to_string(linked.join(".env")).unwrap(),
        "SECRET=1\n"
    );
    assert_eq!(
        fs::read_to_string(linked.join(".herdr/worktree.toml")).unwrap(),
        "[scripts]\n"
    );
    // An existing file is never overwritten, and a tracked one is the
    // worktree's own checkout.
    assert_eq!(
        fs::read_to_string(linked.join("CLAUDE.local.md")).unwrap(),
        "the worktree's own\n"
    );
    assert_eq!(
        fs::read_to_string(linked.join("README.md")).unwrap(),
        "committed\n"
    );
    for absent in [
        ".env.local",
        ".herdr/other.toml",
        "notes.txt",
        "missing.txt",
    ] {
        assert!(!linked.join(absent).exists(), "{absent} was copied");
    }
    // Running again finds everything in place.
    assert!(copy_isolated(&main, &linked).is_empty());
}

#[cfg(not(windows))]
#[test]
fn git_runs_no_fsmonitor_command_the_repository_configures() {
    use std::os::unix::fs::PermissionsExt;
    let (_dir, main, linked) = repository(".env\n");
    write(&main.join(PATH), ".env\n");
    write(&main.join(".env"), "SECRET=1\n");
    // Git runs the setting as a shell command, so its path needs no quoting.
    let plain = tempfile::tempdir().unwrap();
    let (hook, ran) = (plain.path().join("hook"), plain.path().join("ran"));
    write(
        &hook,
        &format!("#!/bin/sh\ntouch \"{}\"\nexit 1\n", ran.display()),
    );
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    git(&main, &["config", "core.fsmonitor", hook.to_str().unwrap()]);

    copy_isolated(&main, &linked);

    assert!(!ran.exists(), "the fsmonitor hook ran");
    assert_eq!(
        fs::read_to_string(linked.join(".env")).unwrap(),
        "SECRET=1\n"
    );
}

#[cfg(not(windows))]
#[test]
fn keeps_links_and_modes_and_writes_nothing_outside_the_worktree() {
    use std::os::unix::fs::PermissionsExt;
    let (dir, main, linked) = repository(".env*\nbuild/\n");
    write(&main.join(PATH), ".env*\nbuild/\n");
    write(&main.join(".env.shared"), "shared\n");
    fs::set_permissions(main.join(".env.shared"), fs::Permissions::from_mode(0o600)).unwrap();
    std::os::unix::fs::symlink(".env.shared", main.join(".env")).unwrap();
    write(&main.join("build/cache/.env"), "cached\n");
    // The worktree's `build` leads outside it.
    let outside = dir.path().join("outside");
    fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, linked.join("build")).unwrap();

    copy_isolated(&main, &linked);

    assert_eq!(
        fs::read_link(linked.join(".env")).unwrap(),
        Path::new(".env.shared")
    );
    let mode = fs::metadata(linked.join(".env.shared"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 0);
}

#[cfg(not(windows))]
#[test]
fn a_main_checkout_outside_git_is_reported() {
    let dir = tempfile::tempdir().unwrap();
    let (main, linked) = (dir.path().join("main"), dir.path().join("linked"));
    write(&main.join(PATH), ".env\n");
    fs::create_dir(&linked).unwrap();
    let checkout = Checkout {
        path: linked.to_str().unwrap().into(),
        root: Some(main.to_str().unwrap().into()),
    };
    let result = copy(&ConnectTarget::Local, &checkout);
    assert!(
        matches!(
            result,
            Err(crate::Error::WorktreeInclude(Error::Copy(
                herdr_client::Error::ScriptExit { .. }
            )))
        ),
        "{result:?}"
    );
}

#[test]
fn nothing_is_copied_without_a_list_or_a_separate_main_checkout() {
    let dir = tempfile::tempdir().unwrap();
    let (main, linked) = (dir.path().join("main"), dir.path().join("linked"));
    write(&main.join(".env"), "SECRET=1\n");
    fs::create_dir(&linked).unwrap();
    let (main, linked) = (main.to_str().unwrap(), linked.to_str().unwrap());
    let checkouts = [
        Checkout {
            path: linked.into(),
            root: Some(main.into()),
        },
        Checkout {
            path: linked.into(),
            root: None,
        },
        Checkout {
            path: main.into(),
            root: Some(main.into()),
        },
    ];
    for checkout in checkouts {
        copy(&ConnectTarget::Local, &checkout).unwrap();
    }
    assert_eq!(fs::read_dir(linked).unwrap().count(), 0);
}

#[test]
fn a_wsl_host_is_refused_as_its_scripts_are() {
    let checkout = Checkout {
        path: "/linked".into(),
        root: Some("/main".into()),
    };
    let wsl = ConnectTarget::Wsl {
        distro: "Ubuntu".into(),
        session: "default".into(),
    };
    assert!(matches!(
        copy(&wsl, &checkout),
        Err(crate::Error::WorktreeScriptsUnsupportedHost)
    ));
}

#[cfg(windows)]
#[test]
fn a_windows_client_refuses_a_local_list_it_cannot_copy() {
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("main");
    write(&main.join(PATH), ".env\n");
    let checkout = Checkout {
        path: dir.path().join("linked").to_str().unwrap().into(),
        root: Some(main.to_str().unwrap().into()),
    };
    assert!(matches!(
        copy(&ConnectTarget::Local, &checkout),
        Err(crate::Error::WorktreeInclude(Error::Unsupported))
    ));
}
