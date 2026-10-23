#![allow(clippy::unwrap_used)]
use super::*;
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
};
const KEY: &str =
    "-----BEGIN OPENSSH PRIVATE KEY-----\nfixture\n-----END OPENSSH PRIVATE KEY-----\n";
fn write(path: &Path, value: &str) {
    fs::write(path, value).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
#[test]
fn symlinks_and_shared_directories_are_rejected_before_generation() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("elsewhere");
    write(&target, KEY);
    symlink(&target, dir.path().join("boat_ed25519")).unwrap();
    assert!(matches!(
        snapshot(dir.path(), |_| panic!("must not generate")),
        Err(Error::KeyFile(_))
    ));
    fs::remove_file(dir.path().join("boat_ed25519")).unwrap();
    fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o777)).unwrap();
    assert!(
        matches!(snapshot(dir.path(), |_| panic!("must not generate")),
        Err(Error::KeyFile(source)) if matches!(*source, crate::Error::GitHub(crate::github::Error::CredentialPermissions)))
    );
}
#[test]
fn a_private_snapshot_cannot_change_when_the_source_is_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let original = dir.path().join("boat_ed25519");
    write(&original, KEY);
    let identity = snapshot(dir.path(), |_| panic!("existing key must be reused")).unwrap();
    fs::remove_file(&original).unwrap();
    write(&original, "replacement");
    assert_eq!(fs::read_to_string(identity.path()).unwrap(), KEY);
    assert_eq!(
        fs::metadata(identity.path()).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let owned = identity.path().to_path_buf();
    drop(identity);
    assert!(!owned.exists());
    assert_eq!(fs::read_to_string(original).unwrap(), "replacement");
}
#[test]
fn generation_preserves_another_apps_winning_key_and_rejects_open_permissions() {
    let dir = tempfile::tempdir().unwrap();
    let destination = dir.path().join("boat_ed25519");
    let identity = snapshot(dir.path(), |generated| {
        write(generated, &KEY.replace("fixture", "loser"));
        write(&destination, KEY);
        Ok(())
    })
    .unwrap();
    assert_eq!(fs::read_to_string(identity.path()).unwrap(), KEY);
    assert_eq!(fs::read_to_string(&destination).unwrap(), KEY);
    fs::set_permissions(&destination, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(
        matches!(snapshot(dir.path(), |_| panic!("must not replace")),
        Err(Error::KeyFile(source)) if matches!(*source, crate::Error::GitHub(crate::github::Error::CredentialPermissions)))
    );
}
#[test]
fn newly_generated_keys_remain_private_and_reusable() {
    let dir = tempfile::tempdir().unwrap();
    let snapshot1 = snapshot(dir.path(), |generated| {
        write(generated, KEY);
        Ok(())
    })
    .unwrap();
    let snapshot2 = snapshot(dir.path(), |_| panic!("must reuse the persisted key")).unwrap();
    assert_eq!(fs::read_to_string(snapshot1.path()).unwrap(), KEY);
    assert_eq!(fs::read_to_string(snapshot2.path()).unwrap(), KEY);
    assert_ne!(snapshot1.path(), snapshot2.path());
}

#[test]
fn writable_non_sticky_temp_parents_are_rejected() {
    let parent = tempfile::tempdir().unwrap();
    let child = parent.path().join("private");
    fs::create_dir(&child).unwrap();
    fs::set_permissions(&child, fs::Permissions::from_mode(0o700)).unwrap();
    fs::set_permissions(parent.path(), fs::Permissions::from_mode(0o777)).unwrap();
    assert!(matches!(protected_path(&child), Err(Error::KeyFile(source))
        if matches!(*source, crate::Error::GitHub(crate::github::Error::CredentialPermissions))));
}
