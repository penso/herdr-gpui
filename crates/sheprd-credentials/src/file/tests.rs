#![allow(clippy::unwrap_used)]
use super::*;
use std::os::unix::fs::{PermissionsExt, symlink};

const NAME: &CStr = c"github-credentials";

/// A stand-in record check: a bounded plain token of printable ASCII, as
/// GitHub's own token record is, and never another feature's JSON record.
fn token_record(value: &SecretString) -> Result<()> {
    let text = value.expose_secret();
    if !text.is_empty()
        && text.len() <= 4096
        && !text.starts_with('{')
        && text.bytes().all(|b| b.is_ascii_graphic())
    {
        Ok(())
    } else {
        Err(Error::Permissions)
    }
}

fn read(path: &Path, name: &CStr) -> Result<Option<SecretString>> {
    read_checked(path, name, token_record)
}

fn store(path: &Path, name: &CStr, token: Option<&SecretString>, plaintext: bool) -> Result<()> {
    store_checked(path, name, token, plaintext, token_record)
}

fn write(path: &Path, name: &CStr, token: Option<&SecretString>) -> Result<()> {
    write_checked(path, name, token, token_record)
}

#[test]
fn another_features_record_uses_its_own_check_and_file() {
    let path = tempfile::tempdir().unwrap();
    std::fs::set_permissions(path.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let name = c"coder-credentials";
    let check: fn(&SecretString) -> Result<()> = |value| {
        if value.expose_secret().starts_with("{\"coder\"") {
            Ok(())
        } else {
            Err(Error::Permissions)
        }
    };
    let record: SecretString = r#"{"coder":1}"#.into();
    // The stand-in token check refuses this record; the feature's check decides.
    assert!(store(path.path(), name, Some(&record), true).is_err());
    store_checked(path.path(), name, Some(&record), true, check).unwrap();
    assert_eq!(
        read_checked(path.path(), name, check)
            .unwrap()
            .unwrap()
            .expose_secret(),
        record.expose_secret()
    );
    assert!(read(path.path(), name).is_err());
    assert!(store_checked(path.path(), name, Some(&"other".into()), true, check).is_err());
    assert!(store_checked(path.path(), name, Some(&record), false, check).is_err());
    assert!(!path.path().join("github-credentials").exists());
    store_checked(path.path(), name, None, false, check).unwrap();
    assert!(read_checked(path.path(), name, check).unwrap().is_none());
}

#[test]
fn private_atomic_roundtrip_and_unsafe_files_rejected() {
    let path = std::env::temp_dir().join(format!("herdr-credential-test-{}", std::process::id()));
    std::fs::create_dir(&path).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(path.clone());
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let token: SecretString = "fixture-not-a-real-token".into();
    assert!(read(&path, NAME).unwrap().is_none());
    let error = store(&path, NAME, Some(&token), false).unwrap_err();
    assert!(!error.to_string().contains(token.expose_secret()));
    assert!(read(&path, NAME).unwrap().is_none());
    store(&path, NAME, Some(&token), true).unwrap();
    store(&path, NAME, None, false).unwrap();
    assert!(!path.join("github-credentials").exists());
    assert!(read(&path, NAME).unwrap().is_none());
    write(&path, NAME, Some(&token)).unwrap();
    write(&path, NAME, Some(&token)).unwrap();
    assert_eq!(
        read(&path, NAME).unwrap().unwrap().expose_secret(),
        token.expose_secret()
    );
    assert!(write(&path, NAME, Some(&"invalid\ntoken".into())).is_err());
    assert_eq!(
        read(&path, NAME).unwrap().unwrap().expose_secret(),
        token.expose_secret()
    );
    let file = path.join("github-credentials");
    assert_eq!(std::fs::metadata(&file).unwrap().mode() & 0o777, 0o600);
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert!(read(&path, NAME).is_err());
    assert!(write(&path, NAME, Some(&token)).is_err());
    std::fs::remove_file(&file).unwrap();
    write(&path, NAME, Some(&token)).unwrap();
    let hardlink = path.join("hardlink");
    std::fs::hard_link(&file, &hardlink).unwrap();
    assert!(read(&path, NAME).is_err());
    assert!(write(&path, NAME, Some(&token)).is_err());
    std::fs::remove_file(&hardlink).unwrap();
    std::fs::write(&file, vec![b'x'; LIMIT + 1]).unwrap();
    assert!(read(&path, NAME).is_err());
    assert!(write(&path, NAME, Some(&token)).is_err());
    std::fs::remove_file(&file).unwrap();
    let target = path.join("untouched");
    std::fs::write(&target, b"untouched").unwrap();
    symlink(&target, &file).unwrap();
    assert!(read(&path, NAME).is_err());
    assert!(write(&path, NAME, Some(&token)).is_err());
    assert!(write(&path, NAME, None).is_err());
    assert!(store(&path, NAME, None, false).is_err());
    assert_eq!(std::fs::read(&target).unwrap(), b"untouched");
    std::fs::remove_file(&file).unwrap();
    write(&path, NAME, Some(&token)).unwrap();
    write(&path, NAME, None).unwrap();
    assert!(read(&path, NAME).unwrap().is_none());
    let linked_dir = path.join("linked-directory");
    symlink(&path, &linked_dir).unwrap();
    assert!(read(&linked_dir, NAME).is_err());
    assert!(write(&linked_dir, NAME, Some(&token)).is_err());
    std::fs::create_dir(&file).unwrap();
    assert!(read(&path, NAME).is_err());
    assert!(write(&path, NAME, Some(&token)).is_err());
    assert!(write(&path, NAME, None).is_err());
    std::fs::remove_dir(&file).unwrap();

    let original = path.join("original");
    let moved = path.join("moved");
    std::fs::create_dir(&original).unwrap();
    std::fs::set_permissions(&original, std::fs::Permissions::from_mode(0o700)).unwrap();
    write(&original, NAME, Some(&token)).unwrap();
    let pinned = directory(&original).unwrap();
    std::fs::rename(&original, &moved).unwrap();
    std::fs::create_dir(&original).unwrap();
    std::fs::set_permissions(&original, std::fs::Permissions::from_mode(0o700)).unwrap();
    write(&original, NAME, Some(&"replacement-fixture".into())).unwrap();
    let mut bytes = Zeroizing::new(Vec::with_capacity(4097));
    existing(&pinned, NAME)
        .unwrap()
        .unwrap()
        .take(4097)
        .read_to_end(&mut bytes)
        .unwrap();
    assert_eq!(bytes.as_slice(), token.expose_secret().as_bytes());
    assert_eq!(
        read(&original, NAME).unwrap().unwrap().expose_secret(),
        "replacement-fixture"
    );
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o777)).unwrap();
    assert!(write(&path, NAME, Some(&token)).is_err());
}
