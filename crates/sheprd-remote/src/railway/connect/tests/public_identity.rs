use super::*;

#[test]
fn a_stale_public_half_cannot_reconnect_to_another_agents_vm() {
    if !tools() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let (selected, id) = key(dir.path(), "selected");
    let (other, _) = key(dir.path(), "other");
    fs::copy(keys::public_path(&other), keys::public_path(&selected)).unwrap();
    let ssh = stand_in::install_script("echo ran >> '@LOG@'; exit 255");
    assert!(matches!(
        connect(dir.path(), &selected, &id),
        Err(Error::PublicKeyMismatch(path)) if path == selected
    ));
    assert!(ssh.runs().is_empty(), "no SSH authentication may start");
}
