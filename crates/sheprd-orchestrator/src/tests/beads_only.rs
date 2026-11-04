use super::*;
use crate::{repo, service::sources};
use sheprd_client::ConnectTarget;
use std::sync::atomic::AtomicBool;

#[test]
fn a_beads_folder_outside_git_is_a_project() {
    let dir = tempfile::tempdir().unwrap();
    let checkout = dir.path().join("notes");
    std::fs::create_dir_all(checkout.join(".beads")).unwrap();
    let host = sheprd_remote::teleport::Host::new(&ConnectTarget::Local).unwrap();
    let info = repo::detect(
        &ConnectTarget::Local,
        &host,
        &checkout.to_string_lossy(),
        &AtomicBool::new(false),
    )
    .unwrap();
    let root = repo::plain(checkout.canonicalize().unwrap());
    assert_eq!(info.repository, Repository::Local(root.join(".beads")));
    assert_eq!(info.main_root, root.to_string_lossy());
    assert!(info.remote.is_none());
    let source = info.beads_source().unwrap();
    assert_eq!(source.provider, Provider::Beads);
    assert_eq!(source.repository, info.main_root);
    assert_eq!(sources(&info), vec![(source, true)]);
}

#[test]
fn ssh_beads_only_projects_have_no_git_source() {
    let target = ConnectTarget::Ssh {
        target: "devbox".into(),
        session: "default".into(),
    };
    let info = repo::parse(
        &target,
        "common\t/projects/notes/.beads\nmain\t/projects/notes\nbeads\t1\n",
    )
    .unwrap();
    assert_eq!(
        info.repository,
        Repository::Ssh {
            destination: "devbox".into(),
            git_dir: "/projects/notes/.beads".into(),
        }
    );
    assert!(info.remote.is_none());
    assert_eq!(sources(&info), vec![(info.beads_source().unwrap(), true)]);
}

#[test]
fn a_file_named_beads_does_not_make_a_project() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join(".beads"), "").unwrap();
    let host = sheprd_remote::teleport::Host::new(&ConnectTarget::Local).unwrap();
    assert!(matches!(
        repo::detect(
            &ConnectTarget::Local,
            &host,
            &dir.path().to_string_lossy(),
            &AtomicBool::new(false),
        ),
        Err(Error::NotARepository)
    ));
}
