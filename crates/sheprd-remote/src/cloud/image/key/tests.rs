use super::*;

fn file(path: &str, bytes: &[u8]) -> File {
    File {
        path: path.into(),
        executable: false,
        bytes: bytes.to_vec(),
    }
}

#[test]
fn clones_share_a_project_and_others_do_not() {
    let ssh = Project::new("app", Some("git@github.com:o/app.git"), "local", "/a");
    let https = Project::new("app", Some("https://github.com/o/app"), "ssh:box", "/b");
    assert_eq!(ssh.id, https.id);
    let fork = Project::new("app", Some("git@github.com:me/app.git"), "local", "/a");
    assert_ne!(ssh.id, fork.id);
    // Without a remote, the place is the identity.
    let here = Project::new("app", None, "local", "/a");
    assert_ne!(here.id, Project::new("app", None, "local", "/b").id);
    assert_ne!(here.id, Project::new("app", None, "ssh:box", "/a").id);
    assert_eq!(here.id, Project::new("app", Some(""), "local", "/a").id);
}

#[test]
fn a_slug_is_readable_bounded_and_unique_per_project() {
    let project = Project {
        id: "0123456789abcdef".into(),
        label: "My App_v2 (copy)".into(),
    };
    assert_eq!(project.slug(), "my-app-v2-copy-01234567");
    let long = Project {
        label: "x".repeat(100),
        ..project.clone()
    };
    assert_eq!(long.slug().len(), 24 + 1 + 8);
    let symbols = Project {
        label: "***".into(),
        ..project
    };
    assert_eq!(symbols.slug(), "project-01234567");
}

#[test]
fn any_change_the_build_reads_changes_the_key() {
    let files = [file("a", b"1"), file("b", b"2")];
    let base = key("FROM x", &files, "linux/arm64");
    assert_eq!(base, key("FROM x", &files, "linux/arm64"));
    assert_eq!(base.len(), 64);
    assert_ne!(base, key("FROM y", &files, "linux/arm64"));
    assert_ne!(base, key("FROM x", &files, "linux/amd64"));
    assert_ne!(
        base,
        key("FROM x", &[file("a", b"1"), file("b", b"3")], "linux/arm64")
    );
    // Boundaries are part of the hash: moving a byte between files counts.
    assert_ne!(
        base,
        key("FROM x", &[file("a", b"12"), file("b", b"")], "linux/arm64")
    );
    let mut executable = files.clone();
    executable[0].executable = true;
    assert_ne!(base, key("FROM x", &executable, "linux/arm64"));
    assert_eq!(short(&base).len(), 12);
}
