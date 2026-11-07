//! Where Sheprd keeps a saved sign-in: the macOS login Keychain in a signed
//! release, the Linux Secret Service, Windows Credential Manager, or, with an
//! explicit opt-in, a private `0600` file. GitHub and the cloud providers keep
//! their secrets here under the same policy. Every call may block on the
//! system's keyring, so callers run it on a background worker, never on the
//! UI thread.

#[cfg(windows)]
mod credential_manager;
mod error;
pub mod file;
#[cfg(target_os = "macos")]
mod keychain;
#[cfg(target_os = "linux")]
mod secret_service;

#[cfg(windows)]
pub use credential_manager::Error as CredentialManagerError;
pub use error::{Error, Result};

use secrecy::SecretString;
use std::{ffi::CStr, path::Path};
use zeroize::Zeroizing;

/// The largest saved record, in bytes. A file any larger is refused unread.
pub const LIMIT: usize = 64 * 1024;

/// Only the signed release pipeline sets `HERDR_RELEASE_VERSION`. Local and
/// worktree builds are unsigned, so features that depend on a stable code
/// identity, macOS Keychain access above all, treat them as development
/// builds.
pub const RELEASE_BUILD: bool = option_env!("HERDR_RELEASE_VERSION").is_some();

// A macOS development build is unsigned and gets a fresh code identity on every
// rebuild, so macOS would re-prompt for the Keychain item's ACL on every run.
// Only the signed release pipeline sets `HERDR_RELEASE_VERSION`, so only it uses
// Keychain. The Secret Service grants access per user session, not per binary,
// so every Linux build uses it, and Credential Manager grants it per Windows
// account, so every Windows build uses that.
pub const KEYRING: bool =
    (cfg!(target_os = "macos") && RELEASE_BUILD) || cfg!(target_os = "linux") || cfg!(windows);
// The private file relies on POSIX ownership and mode bits, so platforms
// without them keep the environment as their only source of a saved token.
pub const FILE: bool = cfg!(unix);
// macOS development builds have no other secure store to fall back on, so the
// private file is their default.
pub const FILE_DEFAULT: bool = cfg!(target_os = "macos") && !KEYRING;
// macOS picks its store from the build alone. Elsewhere the plaintext opt-in
// chooses the file, even over the Secret Service: it is the fallback for a
// desktop without one, and keeps a sign-in saved under that opt-in readable.
pub const OPT_IN: bool = FILE && !cfg!(target_os = "macos");
// An unsigned build must never reach the Keychain, whatever else changes here.
const _: () = assert!(!(KEYRING && cfg!(target_os = "macos")) || RELEASE_BUILD);

/// Where a saved sign-in lives for this build.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Store {
    /// No persistent store: the environment only.
    #[default]
    Environment,
    /// The app-specific macOS login Keychain, Linux Secret Service, or
    /// Windows Credential Manager entry.
    Keyring,
    /// A private `0600` file beside the GUI config.
    File,
}

impl Store {
    /// The store a build with `keyring` and `file_default` uses, given the
    /// user's plaintext opt-in.
    pub const fn choose(opted_in: bool, keyring: bool, file_default: bool) -> Self {
        if opted_in || file_default {
            Self::File
        } else if keyring {
            Self::Keyring
        } else {
            Self::Environment
        }
    }

    /// The store for a feature's credential, from its own plaintext opt-in.
    pub const fn for_policy(allow_plaintext: bool) -> Self {
        Self::choose(allow_plaintext && OPT_IN, KEYRING, FILE_DEFAULT)
    }
}

/// A credential a feature saves: the keyring service, account and label, the
/// private file's name, and the record check every read and write must pass,
/// which must also bound its size. The check returns the feature's own error,
/// which this crate's errors convert into.
pub struct Entry<E> {
    // Only the keyrings address entries by these.
    #[cfg_attr(
        not(any(target_os = "macos", target_os = "linux", windows)),
        allow(dead_code)
    )]
    pub service: &'static str,
    #[cfg_attr(
        not(any(target_os = "macos", target_os = "linux", windows)),
        allow(dead_code)
    )]
    pub account: &'static str,
    /// Shown by the Linux Secret Service and as the Credential Manager
    /// comment; the Keychain has no item label here.
    #[cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]
    pub label: &'static str,
    pub file: &'static CStr,
    pub validate: fn(&SecretString) -> Result<(), E>,
}

/// Reads `entry` from `store`; `directory` holds the private file.
pub fn read_entry<E: From<Error>>(
    store: Store,
    entry: &Entry<E>,
    directory: &Path,
) -> Result<Option<SecretString>, E> {
    match store {
        Store::Environment => Ok(None),
        Store::Keyring => Ok(keyring_read(entry.service, entry.account)?),
        Store::File => file::read_checked(directory, entry.file, entry.validate),
    }
}

/// Writes or removes `entry` in `store`; `directory` holds the private file.
pub fn save_entry<E: From<Error>>(
    store: Store,
    entry: &Entry<E>,
    directory: &Path,
    token: Option<&SecretString>,
) -> Result<(), E> {
    if let Some(token) = token {
        (entry.validate)(token)?;
    }
    match store {
        Store::Keyring => Ok(keyring_save(
            entry.service,
            entry.label,
            token,
            entry.account,
        )?),
        // Removal stays allowed without an opt-in, as for GitHub sign-out.
        Store::Environment => {
            file::store_checked(directory, entry.file, token, false, entry.validate)
        }
        Store::File => file::store_checked(directory, entry.file, token, true, entry.validate),
    }
}

/// The keyring entry `account` under `service`, or `None` when nothing is
/// saved or the platform has no keyring.
pub fn keyring_read(service: &str, account: &str) -> Result<Option<SecretString>> {
    #[cfg(target_os = "macos")]
    return keychain::read(service, account);
    #[cfg(target_os = "linux")]
    return secret_service::read(service, account);
    #[cfg(windows)]
    return credential_manager::read(service, account);
    // `Store::Keyring` is never selected elsewhere.
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = (service, account);
        Ok(None)
    }
}

/// Saves `token` as the keyring entry `account` under `service`, shown as
/// `label` where the keyring shows one, or removes it when `token` is `None`.
pub fn keyring_save(
    service: &str,
    label: &str,
    token: Option<&SecretString>,
    account: &str,
) -> Result<()> {
    #[cfg(target_os = "macos")]
    return {
        let _ = label;
        keychain::save(service, account, token)
    };
    #[cfg(target_os = "linux")]
    return secret_service::save(service, label, token, account);
    #[cfg(windows)]
    return credential_manager::save(service, label, token, account);
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = (service, label, token, account);
        Err(Error::Policy)
    }
}

/// A secret from bytes a keyring returned, which are wiped once copied.
/// Invalid UTF-8 is refused without echoing the bytes.
fn credential_bytes(bytes: Vec<u8>) -> Result<SecretString> {
    let bytes = Zeroizing::new(bytes);
    // Validate by borrowing so invalid UTF-8 never escapes in FromUtf8Error.
    std::str::from_utf8(&bytes)
        .map(SecretString::from)
        .map_err(Error::Encoding)
}

#[cfg(test)]
mod tests;
