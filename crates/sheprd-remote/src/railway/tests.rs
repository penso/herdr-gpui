//! What the Railway tests share: whether `ssh-keygen` is here to drive, and a
//! user's own key to use.
#![allow(clippy::unwrap_used)]
use std::{
    path::{Path, PathBuf},
    process::Command,
};

/// Whether `ssh-keygen` can run here; the tests that need it skip otherwise.
pub(super) fn tools() -> bool {
    Command::new("ssh-keygen").arg("-?").output().is_ok()
}

/// A key `ssh-keygen` made at `dir/name`, and the device ID it names.
pub(super) fn key(dir: &Path, name: &str) -> (PathBuf, String) {
    let key = dir.join(name);
    let made = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(&key)
        .status();
    assert!(made.is_ok_and(|status| status.success()), "ssh-keygen");
    let id = super::keys::fingerprint(&key, &|| false).unwrap();
    (key, id)
}
