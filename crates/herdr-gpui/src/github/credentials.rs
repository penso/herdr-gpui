//! Opt-in Unix plaintext storage. All callers run on a background worker.
#![forbid(unsafe_code)]

use super::Result;
use crate::Error;
use secrecy::SecretString;
use std::{ffi::CStr, path::Path};

#[cfg(unix)]
use super::token::{Credential, LIMIT};
#[cfg(unix)]
use rustix::fs::{AtFlags, Mode, OFlags, open, openat, renameat, unlinkat};
#[cfg(unix)]
use rustix::process::geteuid;
#[cfg(unix)]
use secrecy::ExposeSecret;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
#[cfg(unix)]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(unix)]
use std::{
    fs::File,
    io::{Read, Write},
};
#[cfg(unix)]
use zeroize::Zeroizing;

/// Keeping a token private on disk here depends on `openat`, POSIX ownership,
/// and mode bits. Off POSIX there is no equivalent this crate can rely on, so
/// `Store::File` is never selected and only removal, which has nothing to
/// remove, succeeds.
#[cfg(not(unix))]
pub(super) fn read(_path: &Path, _name: &CStr) -> Result<Option<SecretString>> {
    Ok(None)
}

#[cfg(not(unix))]
pub(super) fn read_checked(
    _path: &Path,
    _name: &CStr,
    _validate: fn(&SecretString) -> Result<()>,
) -> Result<Option<SecretString>> {
    Ok(None)
}

#[cfg(not(unix))]
pub(super) fn store_checked(
    path: &Path,
    name: &CStr,
    token: Option<&SecretString>,
    plaintext: bool,
    _validate: fn(&SecretString) -> Result<()>,
) -> Result<()> {
    store(path, name, token, plaintext)
}

#[cfg(not(unix))]
pub(super) fn store(
    _path: &Path,
    _name: &CStr,
    token: Option<&SecretString>,
    _plaintext: bool,
) -> Result<()> {
    if token.is_some() {
        return Err(Error::CredentialUnsupported);
    }
    Ok(())
}

#[cfg(unix)]
fn directory(path: &Path) -> Result<File> {
    let dir = File::from(
        open(
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|error| Error::CredentialIo(error.into()))?,
    );
    let metadata = dir.metadata().map_err(Error::CredentialIo)?;
    if metadata.uid() != geteuid().as_raw() || metadata.mode() & 0o022 != 0 {
        return Err(Error::CredentialPermissions);
    }
    Ok(dir)
}

#[cfg(unix)]
fn existing(dir: &File, name: &CStr) -> Result<Option<File>> {
    // Keep access relative to the validated directory, even if its path is replaced.
    let file = match openat(
        dir,
        name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => File::from(fd),
        Err(rustix::io::Errno::NOENT) => return Ok(None),
        Err(error) => return Err(Error::CredentialIo(error.into())),
    };
    let metadata = file.metadata().map_err(Error::CredentialIo)?;
    if !metadata.is_file()
        || metadata.uid() != geteuid().as_raw()
        || metadata.mode() & 0o777 != 0o600
        || metadata.nlink() != 1
        || metadata.len() > LIMIT as u64
    {
        return Err(Error::CredentialPermissions);
    }
    Ok(Some(file))
}

#[cfg(unix)]
pub(super) fn read(path: &Path, name: &CStr) -> Result<Option<SecretString>> {
    read_checked(path, name, github_record)
}

#[cfg(unix)]
fn github_record(value: &SecretString) -> Result<()> {
    Credential::decode(value).map(drop)
}

/// `read` for a record another feature owns; `validate` must bound its size.
#[cfg(unix)]
pub(super) fn read_checked(
    path: &Path,
    name: &CStr,
    validate: fn(&SecretString) -> Result<()>,
) -> Result<Option<SecretString>> {
    let dir = directory(path)?;
    let Some(file) = existing(&dir, name)? else {
        return Ok(None);
    };
    let mut bytes = Zeroizing::new(Vec::with_capacity(LIMIT + 1));
    file.take((LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(Error::CredentialIo)?;
    let text = std::str::from_utf8(&bytes).map_err(Error::GitHubEncoding)?;
    let value = SecretString::from(text);
    validate(&value)?;
    Ok(Some(value))
}

#[cfg(unix)]
pub(super) fn store(
    path: &Path,
    name: &CStr,
    token: Option<&SecretString>,
    plaintext: bool,
) -> Result<()> {
    if token.is_some() && !plaintext {
        return Err(Error::CredentialPolicy);
    }
    // Explicit removal is allowed even after opting out of plaintext storage.
    write(path, name, token)
}

/// `store` for a record another feature owns; `validate` must bound its size.
#[cfg(unix)]
pub(super) fn store_checked(
    path: &Path,
    name: &CStr,
    token: Option<&SecretString>,
    plaintext: bool,
    validate: fn(&SecretString) -> Result<()>,
) -> Result<()> {
    if token.is_some() && !plaintext {
        return Err(Error::CredentialPolicy);
    }
    write_checked(path, name, token, validate)
}

#[cfg(unix)]
fn write(path: &Path, target: &CStr, token: Option<&SecretString>) -> Result<()> {
    write_checked(path, target, token, github_record)
}

#[cfg(unix)]
fn write_checked(
    path: &Path,
    target: &CStr,
    token: Option<&SecretString>,
    validate: fn(&SecretString) -> Result<()>,
) -> Result<()> {
    let dir = directory(path)?;
    let present = existing(&dir, target)?.is_some();
    let Some(token) = token else {
        if present {
            unlinkat(&dir, target, AtFlags::empty())
                .map_err(|error| Error::CredentialIo(error.into()))?;
        }
        return dir.sync_all().map_err(Error::CredentialIo);
    };
    validate(token)?;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let name = format!(
        ".github-credentials-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let mut file = File::from(
        openat(
            &dir,
            name.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::RUSR | Mode::WUSR,
        )
        .map_err(|error| Error::CredentialIo(error.into()))?,
    );
    let result = (|| {
        file.write_all(token.expose_secret().as_bytes())
            .map_err(Error::CredentialIo)?;
        file.sync_all().map_err(Error::CredentialIo)?;
        existing(&dir, target)?;
        // Replace the directory entry atomically, never a symlink's target.
        renameat(&dir, name.as_str(), &dir, target)
            .map_err(|error| Error::CredentialIo(error.into()))?;
        dir.sync_all().map_err(Error::CredentialIo)
    })();
    let _ = unlinkat(&dir, name.as_str(), AtFlags::empty());
    result
}

#[cfg(all(test, unix))]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    const NAME: &CStr = c"github-credentials";

    #[test]
    fn another_features_record_uses_its_own_check_and_file() {
        let path = tempfile::tempdir().unwrap();
        std::fs::set_permissions(path.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let name = c"coder-credentials";
        let check: fn(&SecretString) -> Result<()> = |value| {
            if value.expose_secret().starts_with("{\"coder\"") {
                Ok(())
            } else {
                Err(Error::CredentialPermissions)
            }
        };
        let record: SecretString = r#"{"coder":1}"#.into();
        // GitHub's own check would refuse this record; the feature's check decides.
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
    fn accounts_are_separate_files_and_removing_one_keeps_the_other() {
        let path = tempfile::tempdir().unwrap();
        std::fs::set_permissions(path.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let host = c"github-credentials-0123456789abcdef0123456789abcdef";
        let token = |access: &str| {
            Credential::new(access.into(), None, "client")
                .unwrap()
                .encode()
                .unwrap()
        };
        store(path.path(), NAME, Some(&token("main")), true).unwrap();
        store(path.path(), host, Some(&token("host")), true).unwrap();
        let access = |name| {
            Credential::decode(&read(path.path(), name).unwrap().unwrap())
                .unwrap()
                .access_token
                .expose_secret()
                .to_owned()
        };
        assert_eq!(access(NAME), "main");
        assert_eq!(access(host), "host");
        store(path.path(), host, None, false).unwrap();
        assert!(read(path.path(), host).unwrap().is_none());
        assert_eq!(access(NAME), "main");
    }

    #[test]
    fn refresh_record_roundtrips_maximum_escaped_tokens_in_a_private_file() {
        let path = tempfile::tempdir().unwrap();
        std::fs::set_permissions(path.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let access = "\"\\".repeat(2048);
        let refresh = "\\\"".repeat(2048);
        let client = "c".repeat(256);
        let credential = Credential::new(
            access.as_str().into(),
            Some(refresh.as_str().into()),
            &client,
        )
        .unwrap();
        let encoded = credential.encode().unwrap();
        // Every token character needs JSON escaping; the issuing client is also maximal.
        assert!(encoded.expose_secret().len() > 2 * 4096 * 2);
        assert!(encoded.expose_secret().len() <= LIMIT);
        store(path.path(), NAME, Some(&encoded), true).unwrap();
        let file = path.path().join("github-credentials");
        let metadata = std::fs::metadata(&file).unwrap();
        assert_eq!(metadata.mode() & 0o777, 0o600);
        assert_eq!(metadata.uid(), geteuid().as_raw());
        assert_eq!(metadata.nlink(), 1);
        assert_eq!(metadata.len(), encoded.expose_secret().len() as u64);
        let saved = read(path.path(), NAME).unwrap().unwrap();
        assert_eq!(saved.expose_secret(), encoded.expose_secret());
        let restored = Credential::decode(&saved).unwrap();
        assert_eq!(restored.access_token.expose_secret(), access);
        assert_eq!(
            restored.refresh_token.as_ref().unwrap().expose_secret(),
            refresh
        );
        let record: serde_json::Value = serde_json::from_str(saved.expose_secret()).unwrap();
        assert_eq!(record["client_id"], client);
        assert_eq!(record["version"], 1);
        store(path.path(), NAME, None, false).unwrap();
        assert!(read(path.path(), NAME).unwrap().is_none());
        assert!(!file.exists());
    }

    #[test]
    fn private_atomic_roundtrip_and_unsafe_files_rejected() {
        let path =
            std::env::temp_dir().join(format!("herdr-credential-test-{}", std::process::id()));
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
}
