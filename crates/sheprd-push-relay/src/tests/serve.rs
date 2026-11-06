use crate::{Error, serve::read_secret};
use std::{fs, path::Path};

fn write(path: &Path, text: &str, mode: u32) {
    fs::write(path, text).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }
    #[cfg(not(unix))]
    let _ = mode;
}

#[test]
fn a_secret_is_at_least_32_bytes_of_hex() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("secret");
    write(&path, &format!("{}\n", "ab".repeat(32)), 0o600);
    assert_eq!(read_secret(&path).unwrap(), [0xab; 32]);

    for bad in [
        "",
        &"ab".repeat(31),
        &format!("{}a", "ab".repeat(32)),
        &"zz".repeat(32),
    ] {
        write(&path, bad, 0o600);
        assert!(
            matches!(read_secret(&path), Err(Error::SecretSize { .. })),
            "{bad}"
        );
    }
}

#[test]
fn a_missing_secret_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    assert!(matches!(
        read_secret(&dir.path().join("absent")),
        Err(Error::SecretRead { .. })
    ));
}

#[cfg(unix)]
#[test]
fn a_secret_others_can_read_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("secret");
    write(&path, &"ab".repeat(32), 0o644);
    assert!(matches!(read_secret(&path), Err(Error::NotPrivate { .. })));
}
