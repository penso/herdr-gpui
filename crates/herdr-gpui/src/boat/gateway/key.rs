//! Validate the persistent key through the shared private-file policy, then
//! give OpenSSH a GUI-owned snapshot rather than a replaceable source path.
use super::{Error, Result};
use std::path::Path;

pub(super) struct Identity {
    file: tempfile::NamedTempFile,
    _directory: tempfile::TempDir,
}

impl Identity {
    pub(super) fn path(&self) -> &Path {
        self.file.path()
    }
}

#[cfg(not(unix))]
pub(super) fn snapshot(
    _directory: &Path,
    _generate: impl FnOnce(&Path) -> Result<()>,
) -> Result<Identity> {
    Err(storage(crate::github::Error::CredentialUnsupported.into()))
}

fn storage(source: crate::Error) -> Error {
    Error::KeyFile(Box::new(source))
}

#[cfg(unix)]
fn validate(value: &secrecy::SecretString) -> crate::Result<()> {
    use secrecy::ExposeSecret;
    let text = value.expose_secret();
    if text.len() > 4096 || !text.starts_with("-----BEGIN OPENSSH PRIVATE KEY-----\n") {
        return Err(Error::Field("Boat SSH private key").into());
    }
    Ok(())
}

#[cfg(unix)]
fn protected_path(directory: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    // OpenSSH opens this path later. Refuse ancestors another local user can
    // replace; root-owned sticky directories such as /tmp protect our entries.
    for ancestor in directory.canonicalize().map_err(Error::SshKey)?.ancestors() {
        let metadata = std::fs::metadata(ancestor).map_err(Error::SshKey)?;
        if !metadata.is_dir()
            || (metadata.uid() != 0 && metadata.uid() != rustix::process::geteuid().as_raw())
            || (metadata.mode() & 0o022 != 0 && metadata.mode() & 0o1000 == 0)
        {
            return Err(storage(crate::github::Error::CredentialPermissions.into()));
        }
    }
    Ok(())
}

#[cfg(unix)]
pub(super) fn snapshot(
    directory: &Path,
    generate: impl FnOnce(&Path) -> Result<()>,
) -> Result<Identity> {
    use secrecy::ExposeSecret;
    use std::{fs, io::Write, os::unix::fs::DirBuilderExt};
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)
        .map_err(Error::SshKey)?;
    let read =
        || crate::github::read_private_entry(directory, c"boat_ed25519", validate).map_err(storage);
    let key = if let Some(key) = read()? {
        key
    } else {
        let temporary = tempfile::tempdir_in(directory).map_err(Error::SshKey)?;
        let generated = temporary.path().join("boat_ed25519");
        generate(&generated)?;
        match fs::hard_link(&generated, directory.join("boat_ed25519")) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(Error::SshKey(error)),
        }
        // Drop the generated link before the shared policy checks nlink == 1.
        drop(temporary);
        read()?.ok_or(Error::Field("Boat SSH private key"))?
    };
    let temporary = temporary()?;
    let mut file = tempfile::NamedTempFile::new_in(temporary.path()).map_err(Error::SshKey)?;
    file.write_all(key.expose_secret().as_bytes())
        .map_err(Error::SshKey)?;
    file.flush().map_err(Error::SshKey)?;
    Ok(Identity {
        file,
        _directory: temporary,
    })
}

pub(super) fn temporary() -> Result<tempfile::TempDir> {
    // Use the resolved path for creation and cleanup as well as validation;
    // an alias in an untrusted parent must not redirect either later.
    let root = std::env::temp_dir().canonicalize().map_err(Error::SshKey)?;
    #[cfg(unix)]
    protected_path(&root)?;
    tempfile::tempdir_in(root).map_err(Error::SshKey)
}

#[cfg(all(test, unix))]
mod tests;
