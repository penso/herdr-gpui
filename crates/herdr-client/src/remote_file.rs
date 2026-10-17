//! Copy one file from an SSH host and write it back, over the endpoint's
//! noninteractive SSH trust.
//!
//! A POSIX host runs the scripts under `/bin/sh`; a Windows OpenSSH host runs
//! them under Windows PowerShell 5.1 or later through `-EncodedCommand`. The
//! path travels quoted inside the script text; the file's bytes travel only on
//! the child's stdout (fetch) and stdin (push), never on a command line.
//!
//! Both shells share one exit-code contract: a script that refuses exits with
//! one of the codes below before it streams anything or replaces anything, and
//! [`fetch_remote_file`] and [`push_remote_file`] map those codes to [`Error`]
//! variants. Any other failure, including an SSH failure, is
//! [`Error::Script`].
//!
//! A push writes a private temporary file beside the target and renames it
//! over the target only after checking [`Expect`] against the host's file.
//! Another writer can still slip in between that check and the rename; the
//! host offers no lock that every editor honours.
//!
//! An existing file keeps its permission bits, and a new file gets the mode
//! the host's umask gives one; either way the file becomes the SSH user's. A
//! POSIX host whose `stat` cannot report a mode gets the file overwritten in
//! place instead, which keeps its owner but is not atomic.

use crate::{Result, script::ScriptLimits, script::run_script_command};
use sha2::{Digest, Sha256};
use std::{
    io::{self, Read, Write},
    process::Command,
    sync::atomic::AtomicBool,
    time::Duration,
};

mod posix;
mod powershell;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;

/// The largest file a caller should fetch or push.
pub const MAX_REMOTE_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// Longest interval without progress on any of the child's streams.
const IDLE: Duration = Duration::from_secs(30);

/// The path is a symbolic link (or, on Windows, another reparse point), which
/// a push never replaces.
const SYMLINK_EXIT: i32 = 65;
/// Nothing exists at the path.
const MISSING_EXIT: i32 = 66;
/// The path is a folder, device, or other non-regular file.
const NOT_A_FILE_EXIT: i32 = 67;
/// The file is larger than the fetch's limit.
const TOO_LARGE_EXIT: i32 = 68;
/// The host has no SHA-256 tool to check an expected hash with.
const NO_HASHER_EXIT: i32 = 69;
/// The host's file no longer matches what the push expected.
const CONFLICT_EXIT: i32 = 75;

/// Bytes in the megabyte that [`Error::TooLarge`] reports.
const MB: u64 = 1024 * 1024;

/// The shell an SSH host runs the transfer scripts in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteShell {
    /// `/bin/sh` on Linux, macOS, or another POSIX host.
    Posix,
    /// Windows PowerShell on a Windows OpenSSH host.
    PowerShell,
}

/// What a push requires of the host's file before replacing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expect {
    /// A regular file, not a symbolic link, whose SHA-256 is this digest:
    /// the file as it was fetched.
    Sha256([u8; 32]),
    /// Nothing at the path, not even a dangling symbolic link.
    Absent,
    /// Whatever regular file is there, or none.
    Any,
}

/// A completed fetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fetched {
    /// Bytes written to the output.
    pub bytes: u64,
    /// SHA-256 of those bytes, to pass back as [`Expect::Sha256`].
    pub sha256: [u8; 32],
}

/// Why a fetch or push failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("the host's file changed since it was fetched")]
    Conflict,
    #[error("the file is not on the host")]
    Missing,
    #[error("the host has no SHA-256 tool (sha256sum, shasum, or openssl)")]
    NoHasher,
    #[error("the host's path is not a regular file")]
    NotAFile,
    #[error("could not reach the host's file: {0}")]
    /// Boxed: the crate's root error holds this enum in turn.
    Script(#[source] Box<crate::Error>),
    #[error("the host's path is a symbolic link, which is never replaced")]
    Symlink,
    #[error("the file is larger than {} MB", .limit.div_ceil(MB))]
    TooLarge { limit: u64 },
    // keep-sorted end
}

/// Stream `path` on `target` to `out`, refusing a missing path, a folder or
/// device, and a file larger than `max_bytes`. Symbolic links are followed.
///
/// Blocking: call only on a background worker. Linux and macOS clients only;
/// elsewhere this returns [`crate::Error::ScriptUnsupported`].
///
/// Args:
///     target: A `[user@]host` SSH target.
///     shell: The shell the host runs scripts in.
///     path: The host's absolute path, or one starting `~/` (also `~\` for
///         PowerShell) for the login user's home folder.
///     out: Receives the file's bytes. On failure it may hold a prefix.
///     max_bytes: The largest file accepted.
///     cancelled: Kills the transfer when set.
///
/// Returns:
///     The byte count and SHA-256 of what was written to `out`.
///
/// Errors:
///     [`crate::Error::RemoteFile`] for a refusal or a failed script, or a
///     target or platform error from [`crate::Error`].
pub fn fetch_remote_file(
    target: &str,
    shell: RemoteShell,
    path: &str,
    out: impl Write + Send,
    max_bytes: u64,
    cancelled: &AtomicBool,
) -> Result<Fetched> {
    let script = match shell {
        RemoteShell::Posix => posix::fetch_script(path, max_bytes),
        RemoteShell::PowerShell => powershell::fetch_script(path, max_bytes),
    };
    let command = remote_command(target, shell, &script)?;
    Ok(fetch_with(command, out, max_bytes, cancelled)?)
}

/// Replace `path` on `target` with `input`, only while the host's file still
/// meets `expect`. A symbolic link or non-regular file at the path is never
/// replaced, and the temporary file is removed on every failure.
///
/// Blocking: call only on a background worker. Linux and macOS clients only;
/// elsewhere this returns [`crate::Error::ScriptUnsupported`].
///
/// Args:
///     target: A `[user@]host` SSH target.
///     shell: The shell the host runs scripts in.
///     path: As for [`fetch_remote_file`]. Its folder must exist.
///     expect: What the host's file must be just before the rename.
///     input: The new contents. The caller bounds its length.
///     cancelled: Kills the transfer when set; the host's file is unchanged
///         unless the rename already happened.
///
/// Errors:
///     [`crate::Error::RemoteFile`] for a refusal, a conflict, or a failed
///     script, or a target or platform error from [`crate::Error`].
pub fn push_remote_file(
    target: &str,
    shell: RemoteShell,
    path: &str,
    expect: Expect,
    input: impl Read + Send,
    cancelled: &AtomicBool,
) -> Result<()> {
    let script = match shell {
        RemoteShell::Posix => posix::push_script(path, expect),
        RemoteShell::PowerShell => powershell::push_script(path, expect),
    };
    let command = remote_command(target, shell, &script)?;
    Ok(push_with(command, input, cancelled)?)
}

#[cfg(unix)]
fn remote_command(target: &str, shell: RemoteShell, script: &str) -> Result<Command> {
    match shell {
        RemoteShell::Posix => crate::ssh::script_command(target, script),
        RemoteShell::PowerShell => crate::ssh::powershell_script_command(target, script),
    }
}

/// SSH children need a Unix client, as for every other host script.
#[cfg(not(unix))]
fn remote_command(target: &str, _shell: RemoteShell, _script: &str) -> Result<Command> {
    crate::catalog::validate_target(target)?;
    Err(crate::Error::ScriptUnsupported)
}

/// Run a prepared fetch command, hashing its stdout on the way to `out`.
fn fetch_with(
    command: Command,
    out: impl Write + Send,
    max_bytes: u64,
    cancelled: &AtomicBool,
) -> std::result::Result<Fetched, Error> {
    let mut hashing = Hashing {
        out,
        hasher: Sha256::new(),
    };
    let limits = ScriptLimits {
        output: max_bytes,
        idle: IDLE,
    };
    let bytes = run_script_command(command, io::empty(), &mut hashing, limits, cancelled)
        .map_err(|error| classify(error, Some(max_bytes)))?;
    Ok(Fetched {
        bytes,
        sha256: hashing.hasher.finalize().into(),
    })
}

/// Run a prepared push command with `input` on its stdin. The script prints
/// nothing, so any stdout is a failure.
fn push_with(
    command: Command,
    input: impl Read + Send,
    cancelled: &AtomicBool,
) -> std::result::Result<(), Error> {
    let limits = ScriptLimits {
        output: 0,
        idle: IDLE,
    };
    run_script_command(command, input, io::sink(), limits, cancelled)
        .map(|_| ())
        .map_err(|error| classify(error, None))
}

/// The typed refusal for a script's exit code. `limit` is the fetch's size
/// limit; a push has none, so a stray too-large exit stays a script failure.
fn classify(error: crate::Error, limit: Option<u64>) -> Error {
    let code = match &error {
        crate::Error::ScriptOutputLimit => {
            if let Some(limit) = limit {
                return Error::TooLarge { limit };
            }
            None
        }
        crate::Error::ScriptExit { status, .. } => status.code(),
        _ => None,
    };
    match (code, limit) {
        (Some(SYMLINK_EXIT), _) => Error::Symlink,
        (Some(MISSING_EXIT), _) => Error::Missing,
        (Some(NOT_A_FILE_EXIT), _) => Error::NotAFile,
        (Some(TOO_LARGE_EXIT), Some(limit)) => Error::TooLarge { limit },
        (Some(NO_HASHER_EXIT), _) => Error::NoHasher,
        (Some(CONFLICT_EXIT), _) => Error::Conflict,
        _ => Error::Script(Box::new(error)),
    }
}

/// Lowercase hex, as every host hasher prints a digest.
fn hex(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Writes through to `out`, hashing every byte that reached it.
struct Hashing<W> {
    out: W,
    hasher: Sha256,
}

impl<W: Write> Write for Hashing<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.out.write(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}
