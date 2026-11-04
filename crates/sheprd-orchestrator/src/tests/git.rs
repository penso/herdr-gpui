//! gix reads local repositories as the `git` commands it replaced print
//! them, checked against `git` itself.

use crate::git;
use std::{path::Path, process::Command};

fn run(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "tag.gpgsign=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?}: {output:?}");
    String::from_utf8(output.stdout)
        .unwrap()
        .trim_end()
        .to_owned()
}

/// A repository with one commit at `dir/app`.
fn repository(dir: &Path) -> std::path::PathBuf {
    let main = dir.join("app");
    std::fs::create_dir_all(&main).unwrap();
    run(&main, &["init", "-q"]);
    run(&main, &["commit", "-q", "--allow-empty", "-m", "base"]);
    main
}

#[test]
fn remotes_are_read_as_git_remote_get_url_prints_them() {
    let dir = tempfile::tempdir().unwrap();
    let main = repository(dir.path());
    run(&main, &["remote", "add", "origin", "gh:penso/app.git"]);
    run(
        &main,
        &["remote", "add", "scp", "git@gitlab.com:g/sub/app.git"],
    );
    run(
        &main,
        &["remote", "add", "web", "https://github.com/penso/web"],
    );
    run(&main, &["config", "url.git@github.com:.insteadOf", "gh:"]);
    let found = git::checkout(&main).unwrap().unwrap();
    assert_eq!(found.remotes.len(), 3);
    for (name, url) in &found.remotes {
        assert_eq!(url, &run(&main, &["remote", "get-url", name]), "{name}");
    }
    assert!(
        found
            .remotes
            .contains(&("origin".into(), "git@github.com:penso/app.git".into()))
    );
}

#[test]
fn a_linked_worktree_shares_its_main_checkout_and_common_directory() {
    let dir = tempfile::tempdir().unwrap();
    let main = repository(dir.path());
    let linked = dir.path().join("linked");
    run(
        &main,
        &[
            "worktree",
            "add",
            "-q",
            &linked.to_string_lossy(),
            "-b",
            "side",
        ],
    );
    let sub = linked.join("sub");
    std::fs::create_dir_all(&sub).unwrap();
    let from_main = git::checkout(&main).unwrap().unwrap();
    let from_linked = git::checkout(&sub).unwrap().unwrap();
    assert_eq!(from_linked.common, from_main.common);
    assert_eq!(
        from_linked.main.canonicalize().unwrap(),
        main.canonicalize().unwrap()
    );
    assert!(git::checkout(&dir.path().join("gone")).unwrap().is_none());
}

#[test]
fn agent_branches_and_base_commits_match_git() {
    let dir = tempfile::tempdir().unwrap();
    let main = repository(dir.path());
    for branch in ["agent/b", "agent/a", "other", "agent/c"] {
        run(&main, &["branch", branch]);
    }
    // A packed branch is listed with the loose ones.
    run(&main, &["pack-refs", "--all"]);
    run(&main, &["branch", "agent/d"]);
    assert_eq!(
        git::branches(&main, "agent/", 3).unwrap(),
        ["agent/a", "agent/b", "agent/c"]
    );
    assert_eq!(git::branches(&main, "agent/", 10).unwrap().len(), 4);

    run(&main, &["commit", "-q", "--allow-empty", "-m", "second"]);
    run(&main, &["tag", "-a", "v1", "-m", "annotated", "HEAD~1"]);
    let parse = |spec: &str| run(&main, &["rev-parse", &format!("{spec}^{{commit}}")]);
    for base in ["HEAD", "agent/a", "v1"] {
        assert_eq!(git::commit(&main, base).unwrap(), parse(base), "{base}");
    }
    let head = parse("HEAD");
    assert_eq!(git::commit(&main, &head).unwrap(), head);
    assert!(git::commit(&main, "no-such-branch").is_err());
}

/// Git's global configuration is found through the login environment, not
/// this process's: its `url.insteadOf` and an `includeIf` it names apply, and
/// the repository's own configuration still overrides it.
#[test]
fn global_configuration_comes_from_the_login_environment() {
    let dir = tempfile::tempdir().unwrap();
    let main = repository(dir.path());
    run(&main, &["remote", "add", "origin", "corp:penso/app.git"]);
    run(&main, &["remote", "add", "work", "work:team/app.git"]);
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let included = home.join("included.gitconfig");
    std::fs::write(
        &included,
        "[url \"git@gitlab.com:\"]\n\tinsteadOf = work:\n",
    )
    .unwrap();
    let global = home.join("global.gitconfig");
    let gitdir = main.join(".git").canonicalize().unwrap();
    let gitdir = gitdir.to_string_lossy().replace('\\', "/");
    let gitdir = gitdir.strip_prefix("//?/").unwrap_or(&gitdir).to_owned();
    std::fs::write(
        &global,
        format!(
            "[url \"git@github.com:\"]\n\tinsteadOf = corp:\n\
             [includeIf \"gitdir:{gitdir}\"]\n\tpath = {}\n",
            included.to_string_lossy().replace('\\', "/")
        ),
    )
    .unwrap();
    let mut env = |name: &str| match name {
        "HOME" => Some(home.clone().into_os_string()),
        "GIT_CONFIG_GLOBAL" => Some(global.clone().into_os_string()),
        "GIT_CONFIG_NOSYSTEM" => Some("1".into()),
        _ => None,
    };
    let found = git::checkout_in(&main, &mut env).unwrap().unwrap();
    let url = |name: &str| {
        found
            .remotes
            .iter()
            .find(|(remote, _)| remote == name)
            .map(|(_, url)| url.clone())
            .unwrap()
    };
    assert_eq!(url("origin"), "git@github.com:penso/app.git");
    assert_eq!(url("work"), "git@gitlab.com:team/app.git");

    // Without that environment the rewrites are not seen.
    let mut bare = |name: &str| match name {
        "HOME" => Some(dir.path().join("elsewhere").into_os_string()),
        "GIT_CONFIG_NOSYSTEM" => Some("1".into()),
        _ => None,
    };
    let found = git::checkout_in(&main, &mut bare).unwrap().unwrap();
    assert!(
        found
            .remotes
            .contains(&("origin".into(), "corp:penso/app.git".into()))
    );
}

/// A desktop launch: this process has none of the shell profile's Git
/// settings, and the login shell probed as the app probes it exports
/// `GIT_CONFIG_GLOBAL`. Its rewrite applies with nothing but what that shell
/// exported, as `git` started through `login_env::apply` saw it.
#[cfg(unix)]
#[test]
fn a_desktop_launch_reads_git_config_the_login_shell_exports() {
    let dir = tempfile::tempdir().unwrap();
    let main = repository(dir.path());
    run(&main, &["remote", "add", "origin", "corp:penso/app.git"]);
    let global = dir.path().join("global.gitconfig");
    std::fs::write(&global, "[url \"git@github.com:\"]\n\tinsteadOf = corp:\n").unwrap();
    let shell = dir.path().join("shell");
    crate::test_executable::write(
        &shell,
        format!(
            "#!/bin/sh\n[ \"$1 $2 $3\" = '-l -i -c' ] || exit 9\n\
             export GIT_CONFIG_GLOBAL='{}'\nexport GIT_CONFIG_NOSYSTEM=1\neval \"$4\"\n",
            global.display()
        ),
        0o700,
    )
    .unwrap();
    let login = sheprd_client::login_env::probe_for_tests(&shell).unwrap();
    let mut env = |name: &str| {
        login
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.clone())
    };
    let found = git::checkout_in(&main, &mut env).unwrap().unwrap();
    assert!(
        found
            .remotes
            .contains(&("origin".into(), "git@github.com:penso/app.git".into())),
        "{:?}",
        found.remotes
    );
}
