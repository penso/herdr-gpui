use super::*;
use crate::railway::{
    ssh::stand_in,
    tests::{key, tools},
};

#[test]
fn a_stale_public_half_is_refused_before_provisioning_or_saving() {
    if !tools() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let (selected, _) = key(dir.path(), "selected");
    let (other, _) = key(dir.path(), "other");
    fs::copy(keys::public_path(&other), keys::public_path(&selected)).unwrap();
    let ssh = stand_in::install_script("echo ran >> '@LOG@'; exit 0");
    let result = add_device_in(
        dir.path(),
        AddRequest {
            key: selected.clone(),
            name: "box".into(),
            install: true,
        },
        &|| false,
        &|_| panic!("setup must not start"),
        || Ok(Vec::new()),
        |_| panic!("device must not be saved"),
    );
    assert!(matches!(result, Err(Error::PublicKeyMismatch(path)) if path == selected));
    assert!(ssh.runs().is_empty(), "no VM may be created or installed");
}
