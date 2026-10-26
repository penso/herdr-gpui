/// The detection script runs on SSH hosts from every platform, so a CR from a
/// Windows checkout would reach the remote shell as part of each command.
#[test]
fn the_detection_script_has_unix_line_endings_on_every_platform() {
    assert!(!crate::repo::detection_script("/repo").contains('\r'));
}
