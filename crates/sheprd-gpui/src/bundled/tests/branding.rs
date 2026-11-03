use super::*;

fn helper(path: &Path) {
    std::fs::write(path, b"helper").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}

#[test]
fn renamed_helpers_take_precedence_and_legacy_installations_still_work() {
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("sheprd");
    let legacy = dir.path().join("herdr-acp");
    let current = dir.path().join("sheprd-acp");
    helper(&legacy);
    assert_eq!(
        locate_in("herdr-acp", Some(&exe), None),
        Some(legacy.clone())
    );
    helper(&current);
    assert_eq!(locate_in("herdr-acp", Some(&exe), None), Some(current));
}

#[test]
fn upstream_program_names_are_not_rebranded() {
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("sheprd");
    helper(&dir.path().join("sheprd-core"));
    for name in ["herdr", "herdr-core", "hc"] {
        let upstream = dir.path().join(name);
        helper(&upstream);
        assert_eq!(locate_in(name, Some(&exe), None), Some(upstream));
    }
}
