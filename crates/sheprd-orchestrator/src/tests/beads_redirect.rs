//! Projects whose `.beads/redirect` names one database, read on this machine.
use crate::repo::detect;
use sheprd_client::ConnectTarget;
use sheprd_remote::teleport::Host;
use std::{path::Path, sync::atomic::AtomicBool};

fn source_of(checkout: &Path) -> String {
    let host = Host::new(&ConnectTarget::Local).unwrap();
    let info = detect(
        &ConnectTarget::Local,
        &host,
        checkout.to_str().unwrap(),
        &AtomicBool::new(false),
    )
    .unwrap();
    info.beads_source().unwrap().repository
}

#[test]
fn projects_sharing_a_database_read_one_source() {
    let dir = tempfile::tempdir().unwrap();
    let root = crate::repo::plain(dir.path().canonicalize().unwrap());
    let (owner, git, plain) = (root.join("owner"), root.join("git"), root.join("plain"));
    for project in [&owner, &git, &plain] {
        std::fs::create_dir_all(project.join(".beads")).unwrap();
    }
    gix::init(&git).unwrap();
    for project in [&git, &plain] {
        std::fs::write(project.join(".beads/redirect"), "../owner/.beads\n").unwrap();
    }
    let shared = owner.to_str().unwrap();
    assert_eq!(source_of(&owner), shared);
    assert_eq!(source_of(&git), shared);
    assert_eq!(source_of(&plain), shared);
}

#[test]
fn a_redirect_not_naming_a_beads_folder_leaves_bd_to_follow_it() {
    let dir = tempfile::tempdir().unwrap();
    let root = crate::repo::plain(dir.path().canonicalize().unwrap());
    let project = root.join("project");
    std::fs::create_dir_all(project.join(".beads")).unwrap();
    std::fs::create_dir(root.join("tasks")).unwrap();
    std::fs::write(project.join(".beads/redirect"), "../tasks\n").unwrap();
    assert_eq!(source_of(&project), project.to_str().unwrap());
}
