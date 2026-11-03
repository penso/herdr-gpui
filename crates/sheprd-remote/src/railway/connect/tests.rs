#![allow(clippy::unwrap_used)]
use super::*;
use crate::railway::{
    ssh::stand_in,
    tests::{key, tools},
};
use std::fs;

mod public_identity;
mod teleport;

fn connect(dir: &Path, account: &Path, id: &str) -> Result<Bridge> {
    connect_saved(
        dir,
        &account.display().to_string(),
        id,
        "default",
        &AtomicBool::new(false),
    )
}

#[test]
fn a_device_without_its_key_is_reported_and_never_given_another() {
    if !tools() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    // An account that is not a key path, like one saved before keys were.
    let old = connect_saved(
        dir.path(),
        "railway.new",
        "key-a",
        "default",
        &AtomicBool::new(false),
    );
    assert!(matches!(old, Err(Error::NoIdentity)));

    let (saved, id) = key(dir.path(), "id");
    let (other, _) = key(dir.path(), "other");
    let ssh = stand_in::install_script("echo ran >> '@LOG@'; exit 255");
    // A replaced key file never reaches another VM: ssh does not even run.
    fs::copy(&other, &saved).unwrap();
    fs::copy(keys::public_path(&other), keys::public_path(&saved)).unwrap();
    assert!(matches!(
        connect(dir.path(), &saved, &id),
        Err(Error::KeyChanged(_))
    ));
    // A missing one is reported; nothing is generated in its place.
    fs::remove_file(&saved).unwrap();
    let before = fs::read_dir(dir.path()).unwrap().count();
    assert!(matches!(
        connect(dir.path(), &saved, &id),
        Err(Error::KeyMissing(_))
    ));
    assert_eq!(fs::read_dir(dir.path()).unwrap().count(), before);
    assert!(ssh.runs().is_empty(), "ssh must not run without the key");
}

/// A reconnect is headless every time: only setup ever runs a terminal
/// session, which is what can create a VM.
#[test]
fn a_reconnect_is_headless_every_time() {
    if !tools() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let (saved, id) = key(dir.path(), "id");
    let ssh = stand_in::install_script(
        r#"mode=headless
for argument in "$@"; do [ "$argument" = -tt ] && mode=terminal; done
echo "$mode" >> '@LOG@'
exit 255
"#,
    );
    for _ in 0..3 {
        assert!(matches!(
            connect(dir.path(), &saved, &id),
            Err(Error::Bridge(_))
        ));
    }
    assert_eq!(ssh.runs(), ["headless"; 3]);
}
