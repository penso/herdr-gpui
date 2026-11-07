#![allow(clippy::unwrap_used)]
use super::*;

#[cfg(unix)]
mod inspection;

#[test]
fn checkouts_are_read_with_their_counts_under_the_home_directory() {
    let output = "3\t0\t./code/app\n0\t2\t./.herdr/worktrees/app/lane\tx\nbad line\n1\t1\t/abs\n";
    assert_eq!(
        parse(output),
        vec![
            Checkout {
                path: "~/code/app".into(),
                changed: 3,
                unpushed: 0,
            },
            // A tab in the name stays part of it.
            Checkout {
                path: "~/.herdr/worktrees/app/lane\tx".into(),
                changed: 0,
                unpushed: 2,
            },
        ]
    );
}

#[cfg(unix)]
#[test]
fn the_search_finds_only_checkouts_with_work_found_nowhere_else() {
    use std::process::Command;
    let home = tempfile::tempdir().unwrap();
    let git = |dir: &std::path::Path, args: &[&str]| {
        // The user's own Git config, such as commit signing, stays out.
        let status = Command::new("git")
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", home.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
                "-C",
            ])
            .arg(dir)
            .args(args)
            .output()
            .unwrap();
        assert!(status.status.success(), "{status:?}");
    };
    // Dirty: an untracked file.
    let dirty = home.path().join("code/dirty");
    std::fs::create_dir_all(&dirty).unwrap();
    git(&dirty, &["init", "-q"]);
    std::fs::write(dirty.join("notes.txt"), "x").unwrap();
    // Clean, with no commits.
    let clean = home.path().join("code/clean");
    std::fs::create_dir_all(&clean).unwrap();
    git(&clean, &["init", "-q"]);
    // A checkout in Go's source tree is the user's own.
    let gopath = home.path().join("go/src/example.com/tool");
    std::fs::create_dir_all(&gopath).unwrap();
    git(&gopath, &["init", "-q"]);
    std::fs::write(gopath.join("main.go"), "package main").unwrap();
    // Go's module cache is not.
    let module = home.path().join("go/pkg/mod/example.com/dep");
    std::fs::create_dir_all(&module).unwrap();
    git(&module, &["init", "-q"]);
    std::fs::write(module.join("dep.go"), "package dep").unwrap();
    // A dependency checkout under a skipped directory.
    let dependency = home.path().join(".cargo/git/checkouts/dep");
    std::fs::create_dir_all(&dependency).unwrap();
    git(&dependency, &["init", "-q"]);
    git(&dependency, &["commit", "-q", "--allow-empty", "-m", "dep"]);

    let output = Command::new("/bin/sh")
        .arg("-c")
        .arg(script())
        .env("HOME", home.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let mut found = parse(&String::from_utf8_lossy(&output.stdout));
    found.sort_by(|a, b| a.path.cmp(&b.path));
    assert_eq!(
        found,
        vec![
            Checkout {
                path: "~/code/dirty".into(),
                changed: 1,
                unpushed: 0,
            },
            Checkout {
                path: "~/go/src/example.com/tool".into(),
                changed: 1,
                unpushed: 0,
            },
        ]
    );
}
