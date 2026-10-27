//! Windows Credential Manager as the Windows keyring for saved sign-ins.
//! Each entry is a generic credential named `SERVICE/ACCOUNT`, kept for this
//! user on this computer only (`CRED_PERSIST_LOCAL_MACHINE`, never roamed to
//! other computers in a domain). Windows encrypts it with the user's logon
//! secrets, so other users cannot read it.
//!
//! A credential holds at most `CRED_MAX_CREDENTIAL_BLOB_SIZE` bytes. A larger
//! secret, such as several GitLab accounts in one record, is kept in parts
//! named `SERVICE/ACCOUNT#a1`, `#a2`, and so on, and the entry itself holds
//! only which set of parts is current ([`SPLIT`]). A new secret is written to
//! the other set, `#b1` and on, and the entry switched to it in one write
//! before the old set is removed, so a save that fails or stops midway
//! leaves the previous secret whole. A secret that fits is the entry itself. The calls are synchronous and
//! cheap, but callers still run them on a background worker.
//!
//! `unsafe` is needed because the standard library and the workspace's
//! dependencies have no wrapper for the `Cred*` functions; the maintained
//! wrapper crates pull in a second `windows-sys` version. Each call is in its
//! own block that states what it relies on.

use crate::{Result, credential_bytes};
use secrecy::{ExposeSecret, SecretString};
use std::{io, iter::once};
use windows_sys::Win32::{
    Foundation::ERROR_NOT_FOUND,
    Security::Credentials::{
        CRED_MAX_CREDENTIAL_BLOB_SIZE, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW,
        CredDeleteW, CredFree, CredReadW, CredWriteW,
    },
};
use zeroize::{Zeroize, Zeroizing};

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

/// The most bytes one credential holds.
const PART: usize = CRED_MAX_CREDENTIAL_BLOB_SIZE as usize;
/// The most credentials one secret is split across: enough for the largest
/// record any store keeps ([`crate::LIMIT`]); a larger one is refused.
const PARTS: usize = crate::LIMIT.div_ceil(PART);

/// The credential's target name, which Credential Manager shows and keys on.
fn target(service: &str, account: &str) -> String {
    format!("{service}/{account}")
}

/// Starts an entry that names a split secret's current parts, as
/// `herdr-parts:SET:COUNT`, instead of holding the secret. A secret that
/// starts with it is always split, so the two are never confused.
const SPLIT: &[u8] = b"herdr-parts:";

/// One of the two sets of names a split secret's parts alternate between.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Set {
    A,
    B,
}

impl Set {
    const BOTH: [Self; 2] = [Self::A, Self::B];

    fn letter(self) -> char {
        match self {
            Self::A => 'a',
            Self::B => 'b',
        }
    }

    fn other(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::B => Self::A,
        }
    }
}

/// Where an entry's split secret is: its set of parts and how many.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Split {
    set: Set,
    count: usize,
}

impl Split {
    /// The parts an entry names, or `None` for an entry that is the secret.
    /// An entry that starts like one but names no valid parts is damaged.
    fn of(entry: &[u8]) -> Option<Result<Self>> {
        let rest = entry.strip_prefix(SPLIT)?;
        let parsed = std::str::from_utf8(rest).ok().and_then(|rest| {
            let (set, count) = rest.split_once(':')?;
            let set = match set {
                "a" => Set::A,
                "b" => Set::B,
                _ => return None,
            };
            let count = count
                .parse()
                .ok()
                .filter(|count| (1..=PARTS).contains(count))?;
            Some(Self { set, count })
        });
        Some(parsed.ok_or_else(|| Error::Read(io::ErrorKind::InvalidData.into()).into()))
    }

    fn entry(self) -> String {
        format!(
            "{}{}:{}",
            String::from_utf8_lossy(SPLIT),
            self.set.letter(),
            self.count
        )
    }
}

/// The target of part `index`, from 0, of `set`.
fn part_target(service: &str, account: &str, set: Set, index: usize) -> String {
    format!("{}#{}{}", target(service, account), set.letter(), index + 1)
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

/// The entry `account` under `service`, or `None` when nothing is saved.
/// A split secret is joined from the parts its entry names; a missing part
/// is an error, never a shorter secret.
pub(crate) fn read(service: &str, account: &str) -> Result<Option<SecretString>> {
    let Some(entry) = read_part(&target(service, account))? else {
        return Ok(None);
    };
    let mut entry = Zeroizing::new(entry);
    let split = match Split::of(&entry) {
        None => return credential_bytes(std::mem::take(&mut *entry)).map(Some),
        Some(split) => split?,
    };
    // Sized for every part up front, so no copy is left unwiped by a move.
    let mut secret = Zeroizing::new(Vec::with_capacity(PART * split.count));
    for index in 0..split.count {
        let part = read_part(&part_target(service, account, split.set, index))?
            .ok_or_else(|| Error::Read(io::ErrorKind::NotFound.into()))?;
        secret.extend_from_slice(&Zeroizing::new(part));
    }
    credential_bytes(std::mem::take(&mut *secret)).map(Some)
}

/// One credential's bytes, or `None` when it does not exist.
fn read_part(target: &str) -> Result<Option<Vec<u8>>> {
    let name = wide(target);
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
            Err(Error::Read(error).into())
        };
    }
    Ok(Some(Read(credential).secret()))
}

/// Saves `token` as `account` under `service`, with `label` as its comment,
/// or removes the entry when it is `None`. Removing an entry that does not
/// exist succeeds. Until the entry itself is written, the previous secret
/// stays whole; see the module's note on split secrets.
pub(crate) fn save(
    service: &str,
    label: &str,
    token: Option<&SecretString>,
    account: &str,
) -> Result<()> {
    let entry = target(service, account);
    let Some(token) = token else {
        delete(&wide(&entry))?;
        remove_parts(service, account, &Set::BOTH, 0);
        return Ok(());
    };
    let secret = token.expose_secret().as_bytes();
    if secret.len() <= PART && !secret.starts_with(SPLIT) {
        write_part(&entry, label, account, secret)?;
        remove_parts(service, account, &Set::BOTH, 0);
        return Ok(());
    }
    let parts: Vec<&[u8]> = secret.chunks(PART).collect();
    if parts.len() > PARTS {
        return Err(Error::Write(io::ErrorKind::FileTooLarge.into()).into());
    }
    let current = read_part(&entry)?
        .and_then(|entry| Split::of(&Zeroizing::new(entry)))
        .and_then(Result::ok);
    let split = Split {
        set: current.map_or(Set::A, |current| current.set.other()),
        count: parts.len(),
    };
    let written = parts
        .iter()
        .enumerate()
        .try_for_each(|(index, part)| {
            write_part(
                &part_target(service, account, split.set, index),
                label,
                account,
                part,
            )
        })
        .and_then(|()| write_part(&entry, label, account, split.entry().as_bytes()));
    if let Err(error) = written {
        // The entry still names the previous secret, which is left whole.
        remove_parts(service, account, &[split.set], 0);
        return Err(error);
    }
    remove_parts(service, account, &[split.set.other()], 0);
    remove_parts(service, account, &[split.set], split.count);
    Ok(())
}

/// Removes `sets`' parts from `from` on, which no entry names any more.
/// A part left behind is never read, so a failure here fails no save.
fn remove_parts(service: &str, account: &str, sets: &[Set], from: usize) {
    for &set in sets {
        for index in from..PARTS {
            if let Err(error) = delete(&wide(&part_target(service, account, set, index))) {
                tracing::debug!(%error, "A stale Credential Manager part could not be removed");
            }
        }
    }
}

/// Writes one credential of at most [`PART`] bytes.
fn write_part(target: &str, label: &str, account: &str, secret: &[u8]) -> Result<()> {
    let size = u32::try_from(secret.len())
        .ok()
        .filter(|size| *size <= CRED_MAX_CREDENTIAL_BLOB_SIZE)
        .ok_or_else(|| crate::Error::from(Error::Write(io::ErrorKind::FileTooLarge.into())))?;
    let mut name = wide(target);
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
        return Err(Error::Write(io::Error::last_os_error()).into());
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
            return Err(Error::Write(error).into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
