use super::*;

/// Runs the remote config script locally, as SSH would on the host, never
/// looking for a repository above `ceiling`.
fn read(dir: &Path, ceiling: &Path) -> std::process::Output {
    Command::new("/bin/sh")
        .args([
            "-c",
            &config_command(dir.to_str().unwrap(), "remote.origin.url"),
        ])
        .env("GIT_CEILING_DIRECTORIES", ceiling)
        .output()
        .unwrap()
}

#[test]
fn config_reads_from_a_working_directory_or_its_git_directory() {
    // Herdr reports a Git directory for only some workspaces; the others are
    // read from the workspace directory, which may be below the repository root.
    let root = std::env::temp_dir().join(format!("herdr-config-dir-{}", std::process::id()));
    let nested = root.join("repo/src");
    let outside = root.join("plain");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    let repo = root.join("repo");
    for args in [
        &["init", "-q"][..],
        &["remote", "add", "origin", "git@github.com:owner/repo.git"],
    ] {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&repo)
                .args(args)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    for dir in [repo.clone(), nested, repo.join(".git")] {
        let output = read(&dir, &root);
        assert!(output.status.success(), "{}", dir.display());
        assert_eq!(output.stdout, b"git@github.com:owner/repo.git\n");
    }
    // Outside a repository no remote is found, never another repository's.
    let output = read(&outside, &root);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    std::fs::remove_dir_all(root).unwrap();
}
