#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn frames_parse_and_bad_ones_are_refused() {
    let mut output = Vec::new();
    for (path, mode, bytes) in [
        ("b/run.sh", "x", &b"#!/bin/sh\x00x"[..]),
        ("a.txt", "-", b""),
    ] {
        output.extend_from_slice(format!("{path}\0{mode}\0{}\0", bytes.len()).as_bytes());
        output.extend_from_slice(bytes);
    }
    let files = parse(&output).unwrap();
    assert_eq!(files[0].path, "a.txt");
    assert_eq!(files[1].bytes, b"#!/bin/sh\x00x");
    assert!(files[1].executable);
    assert!(matches!(
        parse(b"a\x00-\x005\x00abc"),
        Err(Error::Truncated)
    ));
    assert!(matches!(parse(b"a\0-\0"), Err(Error::Truncated)));
    assert!(matches!(
        parse(b"../x\x00-\x000\x00"),
        Err(Error::Field(Field::ContextPath))
    ));
    assert!(matches!(
        parse(b"/etc/x\x00-\x000\x00"),
        Err(Error::Field(Field::ContextPath))
    ));
}

#[cfg(unix)]
#[test]
fn only_files_git_would_commit_leave_the_host() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", root)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .arg("-C")
            .arg(root)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success());
    };
    git(&["init", "-q"]);
    let context = root.join(".herdr/sandbox");
    std::fs::create_dir_all(context.join("conf")).unwrap();
    std::fs::write(context.join("Dockerfile"), "FROM x\n").unwrap();
    std::fs::write(context.join("conf/a b.txt"), "spaced").unwrap();
    std::fs::write(context.join("secret.env"), "TOKEN=1").unwrap();
    std::fs::write(context.join(".gitignore"), "secret.env\n").unwrap();
    std::os::unix::fs::symlink("/etc/hosts", context.join("hosts")).unwrap();
    std::fs::write(context.join("setup.sh"), "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(
        context.join("setup.sh"),
        std::os::unix::fs::PermissionsExt::from_mode(0o755),
    )
    .unwrap();
    let host = crate::teleport::Host::new(&herdr_client::ConnectTarget::Local).unwrap();
    let cancelled = AtomicBool::new(false);
    let root = root.to_str().unwrap();
    let everything = read(&host, root, ".herdr/sandbox", &[".".into()], &cancelled).unwrap();
    let paths: Vec<_> = everything.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(
        paths,
        [".gitignore", "Dockerfile", "conf/a b.txt", "setup.sh"]
    );
    assert!(everything[3].executable);
    let some = read(&host, root, ".herdr/sandbox", &["conf".into()], &cancelled).unwrap();
    assert_eq!(some.len(), 1);
    assert_eq!(some[0].bytes, b"spaced");
    let dockerfile = read_one(&host, root, ".herdr/sandbox/Dockerfile", &cancelled).unwrap();
    assert_eq!(dockerfile.unwrap().bytes, b"FROM x\n");
    assert_eq!(
        read_one(&host, root, "missing/Dockerfile", &cancelled).unwrap(),
        None
    );
}

#[cfg(unix)]
#[test]
fn a_linked_folder_cannot_reach_outside_the_project() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let git = |repo: &std::path::Path, args: &[&str]| {
        let status = Command::new("git")
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", dir.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .arg("-C")
            .arg(repo)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success());
    };
    let project = dir.path().join("project");
    let other = dir.path().join("other");
    for repo in [&project, &other] {
        std::fs::create_dir_all(repo).unwrap();
        git(repo, &["init", "-q"]);
    }
    std::fs::write(other.join("private.txt"), "not yours").unwrap();
    std::os::unix::fs::symlink(&other, project.join("context")).unwrap();
    let host = crate::teleport::Host::new(&herdr_client::ConnectTarget::Local).unwrap();
    let cancelled = AtomicBool::new(false);
    let root = project.to_str().unwrap();
    assert!(matches!(
        read(&host, root, "context", &[".".into()], &cancelled),
        Err(Error::Host(_))
    ));
    assert!(read_one(&host, root, "context/private.txt", &cancelled).is_err());
}

#[cfg(unix)]
#[test]
fn a_tracked_file_under_a_folder_now_linked_is_skipped() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let project = dir.path().join("project");
    let outside = dir.path().join("outside");
    let git = |args: &[&str]| {
        let status = Command::new("git")
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", dir.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .arg("-C")
            .arg(&project)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success());
    };
    std::fs::create_dir_all(project.join("context/conf")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    git(&["init", "-q"]);
    std::fs::write(project.join("context/Dockerfile"), "FROM x\n").unwrap();
    std::fs::write(project.join("context/conf/secret.txt"), "tracked").unwrap();
    git(&["add", "."]);
    std::fs::write(outside.join("secret.txt"), "not yours").unwrap();
    std::fs::remove_dir_all(project.join("context/conf")).unwrap();
    std::os::unix::fs::symlink(&outside, project.join("context/conf")).unwrap();
    let host = crate::teleport::Host::new(&herdr_client::ConnectTarget::Local).unwrap();
    let cancelled = AtomicBool::new(false);
    let root = project.to_str().unwrap();
    let files = read(&host, root, "context", &[".".into()], &cancelled).unwrap();
    let paths: Vec<_> = files.iter().map(|file| file.path.as_str()).collect();
    assert_eq!(paths, ["Dockerfile"]);
    assert!(read_one(&host, root, "context/conf/secret.txt", &cancelled).is_err());
}
