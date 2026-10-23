//! Windows Credential Manager as the Windows keyring for saved sign-ins.
//! Each entry is a generic credential named `SERVICE/ACCOUNT`, kept for this
//! user on this computer only (`CRED_PERSIST_LOCAL_MACHINE`, never roamed to
//! other computers in a domain). Windows encrypts it with the user's logon
//! secrets, so other users cannot read it. The calls are synchronous and
//! cheap, but callers still run them on a background worker.
//!
//! `unsafe` is needed because the standard library and the workspace's
//! dependencies have no wrapper for the `Cred*` functions; the maintained
//! wrapper crates pull in a second `windows-sys` version. Each call is in its
//! own block that states what it relies on.

use super::{Result, store::credential_bytes};
use secrecy::{ExposeSecret, SecretString};
use std::{io, iter::once};
use windows_sys::Win32::{
    Foundation::ERROR_NOT_FOUND,
    Security::Credentials::{
        CRED_MAX_CREDENTIAL_BLOB_SIZE, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW,
        CredDeleteW, CredFree, CredReadW, CredWriteW,
    },
};
use zeroize::Zeroize;

const SERVICE: &str = "dev.herdr.gpui.github";
const LABEL: &str = "Herdr UI GitHub sign-in";

/// Why Credential Manager refused, with the Windows error as the source.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "Cannot read a saved sign-in from Windows Credential Manager. Sign in again, or set GH_TOKEN for GitHub."
    )]
    Read(#[source] io::Error),
    #[error("Windows Credential Manager update failed. Try again.")]
    Write(#[source] io::Error),
}

/// The credential's target name, which Credential Manager shows and keys on.
fn target(service: &str, account: &str) -> String {
    format!("{service}/{account}")
}

/// A NUL-terminated UTF-16 copy for the wide-string API.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(once(0)).collect()
}

fn not_found(error: &io::Error) -> bool {
    error.raw_os_error() == i32::try_from(ERROR_NOT_FOUND).ok()
}

/// A credential `CredReadW` allocated. Its secret is wiped before it is freed.
struct Read(*mut CREDENTIALW);

impl Read {
    fn secret(&self) -> Vec<u8> {
        // SAFETY: `CredReadW` succeeded, so `self.0` points to a valid
        // CREDENTIALW whose `CredentialBlob` is null or holds exactly
        // `CredentialBlobSize` bytes, all alive until `CredFree` in `drop`.
        #[allow(unsafe_code)]
        unsafe {
            let credential = &*self.0;
            if credential.CredentialBlob.is_null() {
                return Vec::new();
            }
            std::slice::from_raw_parts(
                credential.CredentialBlob,
                credential.CredentialBlobSize as usize,
            )
            .to_vec()
        }
    }
}

impl Drop for Read {
    fn drop(&mut self) {
        // SAFETY: as in `secret`; the blob is writable memory this process
        // owns, and nothing uses the credential after `CredFree`.
        #[allow(unsafe_code)]
        unsafe {
            let credential = &*self.0;
            if !credential.CredentialBlob.is_null() {
                std::slice::from_raw_parts_mut(
                    credential.CredentialBlob,
                    credential.CredentialBlobSize as usize,
                )
                .zeroize();
            }
            CredFree(self.0.cast_const().cast());
        }
    }
}

/// The saved token for `account`, or `None` when nothing is saved.
pub(super) fn read(account: &str) -> Result<Option<SecretString>> {
    read_in(SERVICE, account)
}

/// `read` for an entry under another feature's service name.
pub(super) fn read_in(service: &str, account: &str) -> Result<Option<SecretString>> {
    let name = wide(&target(service, account));
    let mut credential = std::ptr::null_mut();
    // SAFETY: `name` is NUL-terminated and outlives the call, and
    // `credential` is a valid out-pointer. On success it receives a buffer
    // the system allocated, which `Read` frees exactly once.
    #[allow(unsafe_code)]
    let found = unsafe { CredReadW(name.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) };
    if found == 0 {
        let error = io::Error::last_os_error();
        return if not_found(&error) {
            Ok(None)
        } else {
            Err(super::Error::from(Error::Read(error)).into())
        };
    }
    credential_bytes(Read(credential).secret()).map(Some)
}

/// Saves `token`, or removes the entry when it is `None`. Removing an entry
/// that does not exist succeeds.
pub(super) fn save(token: Option<&SecretString>, account: &str) -> Result<()> {
    save_in(SERVICE, LABEL, token, account)
}

/// `save` for an entry under another feature's service name and label.
pub(super) fn save_in(
    service: &str,
    label: &str,
    token: Option<&SecretString>,
    account: &str,
) -> Result<()> {
    let mut name = wide(&target(service, account));
    let Some(token) = token else {
        return delete(&name);
    };
    let secret = token.expose_secret().as_bytes();
    let size = u32::try_from(secret.len())
        .ok()
        .filter(|size| *size <= CRED_MAX_CREDENTIAL_BLOB_SIZE)
        .ok_or_else(|| {
            crate::Error::from(super::Error::from(Error::Write(
                io::ErrorKind::FileTooLarge.into(),
            )))
        })?;
    let mut comment = wide(label);
    let mut user = wide(account);
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: name.as_mut_ptr(),
        Comment: comment.as_mut_ptr(),
        CredentialBlobSize: size,
        // `CredWriteW` only reads the blob; the field is `*mut` by declaration.
        CredentialBlob: secret.as_ptr().cast_mut(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: user.as_mut_ptr(),
        ..CREDENTIALW::default()
    };
    // SAFETY: every pointer in `credential` refers to a live buffer of the
    // stated length (strings NUL-terminated) that outlives the call, which
    // copies them and writes through none. It replaces an existing entry with
    // the same target in one step.
    #[allow(unsafe_code)]
    let written = unsafe { CredWriteW(&credential, 0) };
    if written == 0 {
        return Err(super::Error::from(Error::Write(io::Error::last_os_error())).into());
    }
    Ok(())
}

fn delete(name: &[u16]) -> Result<()> {
    // SAFETY: `name` comes from `wide`, so it is NUL-terminated, and it
    // outlives the call.
    #[allow(unsafe_code)]
    let deleted = unsafe { CredDeleteW(name.as_ptr(), CRED_TYPE_GENERIC, 0) };
    if deleted == 0 {
        let error = io::Error::last_os_error();
        if !not_found(&error) {
            return Err(super::Error::from(Error::Write(error)).into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
