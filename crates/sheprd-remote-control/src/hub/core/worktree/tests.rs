use super::*;
use tempfile::tempdir;

fn run_git(repository: &Path, args: &[&str]) {
    let status = new_command("git")
        .current_dir(repository)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

pub(in crate::hub) fn committed_repository(parent: &Path) -> PathBuf {
    let repository = parent.join("project");
    std::fs::create_dir(&repository).unwrap();
    run_git(&repository, &["init", "--quiet"]);
    run_git(
        &repository,
        &["commit", "--quiet", "--allow-empty", "-m", "start"],
    );
    repository
}

#[test]
fn a_new_branch_is_checked_out_where_herdr_would_put_it() {
    let dir = tempdir().unwrap();
    let repository = committed_repository(dir.path());
    let root = dir.path().join("worktrees");
    let checkout = add(&repository, "worktree/Quiet_forest-a09d", &root).unwrap();
    assert_eq!(
        checkout,
        root.join("project").join("worktree-quiet-forest-a09d")
    );
    let head = git(
        &checkout,
        [OsStr::new("branch"), OsStr::new("--show-current")],
    )
    .unwrap();
    assert_eq!(head.trim(), "worktree/Quiet_forest-a09d");
}

#[test]
fn a_worktree_of_a_worktree_files_under_the_same_repository() {
    let dir = tempdir().unwrap();
    let repository = committed_repository(dir.path());
    let root = dir.path().join("worktrees");
    let first = add(&repository, "one", &root).unwrap();
    assert_eq!(
        add(&first, "two", &root).unwrap(),
        root.join("project").join("two")
    );
}

#[test]
fn a_taken_branch_is_refused_with_what_git_said() {
    let dir = tempdir().unwrap();
    let repository = committed_repository(dir.path());
    let root = dir.path().join("worktrees");
    add(&repository, "topic", &root).unwrap();
    let error = add(&repository, "topic", &dir.path().join("other")).unwrap_err();
    let Error::Refused(complaint) = error else {
        panic!("{error}");
    };
    assert!(complaint.contains("topic"), "{complaint}");
}

#[test]
fn a_folder_outside_any_repository_is_refused() {
    let dir = tempdir().unwrap();
    let error = add(dir.path(), "topic", &dir.path().join("worktrees")).unwrap_err();
    assert!(matches!(error, Error::Refused(_)), "{error}");
}

#[test]
fn a_bare_repository_is_named_without_its_suffix() {
    assert_eq!(
        repository_name(Path::new("/code/project.git")),
        Some(OsStr::new("project"))
    );
    assert_eq!(
        repository_name(Path::new("/code/project/.git")),
        Some(OsStr::new("project"))
    );
}

#[test]
fn a_branch_with_nothing_to_keep_gets_the_fallback_folder() {
    assert_eq!(path_slug("--/--"), FALLBACK_SLUG);
    assert_eq!(path_slug("--Fix_42--"), "fix-42");
}
