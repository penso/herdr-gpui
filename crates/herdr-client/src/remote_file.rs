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
//! A push writes a temporary file beside the target and renames it over the
//! target only after checking [`Expect`] against the host's file. The
//! temporary file never lets anyone read the upload who cannot read the file
//! its access comes from: the target when it exists, else the file
//! [`Access::Like`] names. A POSIX host creates it readable by the SSH user
//! alone (but see the macOS limitation below); Windows PowerShell and
//! PowerShell 7 create it with that file's DACL.
//! Another writer can still slip in between that check and the rename; the
//! host offers no lock that every editor honours.
//!
//! An existing file keeps its permission bits and, on a POSIX host, its
//! group, and becomes the SSH user's. A set-user-ID bit is not kept when the
//! file's owner changes, nor a set-group-ID bit when its group cannot be
//! kept: the file then grants no identity it did not grant before. On a
//! POSIX host it is overwritten in place instead, keeping its owner, group,
//! and ACL but not atomically, when the host cannot report its mode or
//! group, when an ACL shows on the file or on the temporary file, or when
//! the copy cannot be given the file's group
//! and the group and other bits differ. When it would write in place over a
//! file the SSH user cannot write, it refuses with [`Error::Access`] before
//! the host's file changes. It writes in place only in a folder where no one
//! but the SSH user and root can replace the file (the folder is theirs,
//! carries no ACL, and neither group nor others may write it unless it is
//! sticky and the file is theirs); elsewhere it refuses with
//! [`Error::SharedFolder`], and a push expecting nothing at the path refuses
//! with [`Error::Conflict`] rather than write over a file that appeared.
//! The POSIX copy is built in a private folder inside the target's folder,
//! so no one else can swap it for a link before it is renamed.
//!
//! A new file takes the access of the file [`Access::Like`] names, as a
//! kept-both copy does, or with [`Access::Keep`] what the host gives a new
//! file. On a POSIX host a new file whose source's access cannot be copied
//! (the source is missing, a link, not a regular file, or carries an ACL, or
//! its group cannot be given and its group and other bits differ) is never
//! refused: it stays readable by the SSH user alone. PowerShell refuses with
//! [`Error::Access`] instead when it cannot read the source's DACL or the
//! source is not a file.
//!
//! Limitation: on a macOS host, an inheritable ACL entry on the folder can
//! let others read the temporary file while the upload is written, since
//! macOS does not mask ACL entries by the file's 0600 mode. The final file is
//! still right, because the ACL check then writes the upload in place.

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
/// The pushed input was not the length the caller said it would be: the
/// transfer was cut off, or the file changed while it was read.
const SHORT_EXIT: i32 = 70;
/// The push cannot keep the file's access: it would write in place over a
/// target the SSH user cannot write, or PowerShell cannot read the ACL to
/// copy or its source is not a file. The host's file was not changed.
const ACCESS_EXIT: i32 = 71;
/// Users other than the SSH user and root can replace files in the target's
/// folder, so a POSIX push that could only write in place did not, or the
/// push's private work folder was not the SSH user's alone. The host's file
/// was not changed.
const SHARED_FOLDER_EXIT: i32 = 72;
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

/// Where a pushed file's access comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access<'a> {
    /// An existing target keeps its own access; a new file gets what the
    /// host gives one: the umask's mode, or the folder's inherited ACL.
    Keep,
    /// A new file takes the access of the host file at this path, written as
    /// a push's path is. An existing target still keeps its own access.
    Like(&'a str),
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
    #[error("could not keep the file's access; nothing was changed")]
    Access,
    #[error("the host's file changed since it was fetched")]
    Conflict,
    #[error("the upload was cut off; the host's file is unchanged")]
    Incomplete,
    #[error("the file is not on the host")]
    Missing,
    #[error("the host has no SHA-256 tool (sha256sum, shasum, or openssl)")]
    NoHasher,
    #[error("the host's path is not a regular file")]
    NotAFile,
    #[error("could not reach the host's file: {0}")]
    /// Boxed: the crate's root error holds this enum in turn.
    Script(#[source] Box<crate::Error>),
    #[error(
        "other users can replace files in that folder on the host, so it was not written in \
         place; nothing was changed"
    )]
    SharedFolder,
    #[error("the host's path is a symbolic link, which is never replaced")]
    Symlink,
    #[error("the file is larger than {} MB", .limit.div_ceil(MB))]
    TooLarge { limit: u64 },
    // keep-sorted end
}

/// A file on an SSH host: where it is and the shell its scripts run in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteFile<'a> {
    /// A `[user@]host` SSH target.
    pub target: &'a str,
    /// The shell the host runs scripts in.
    pub shell: RemoteShell,
    /// The host's absolute path, or one starting `~/` (also `~\` for
    /// PowerShell) for the login user's home folder.
    pub path: &'a str,
}

/// Stream `file` to `out`, refusing a missing path, a folder or device, and a
/// file larger than `max_bytes`. Symbolic links are followed.
///
/// Blocking: call only on a background worker.
///
/// Args:
///     file: The host, its shell, and the path to read.
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
    file: RemoteFile<'_>,
    out: impl Write + Send,
    max_bytes: u64,
    cancelled: &AtomicBool,
) -> Result<Fetched> {
    let script = match file.shell {
        RemoteShell::Posix => posix::fetch_script(file.path, max_bytes),
        RemoteShell::PowerShell => powershell::fetch_script(file.path, max_bytes),
    };
    let command = remote_command(file.target, file.shell, &script)?;
    Ok(fetch_with(command, out, max_bytes, cancelled)?)
}

/// Replace `file` with `input`, only while the host's file still meets
/// `expect`. A symbolic link or non-regular file at the path is never
/// replaced, and the temporary file is removed on every failure.
///
/// Blocking: call only on a background worker.
///
/// Args:
///     file: The host, its shell, and the path to replace. The path's folder
///         must exist.
///     expect: What the host's file must be just before the rename.
///     access: Where a new file's access comes from.
///     len: The exact number of bytes `input` holds. The host replaces
///         nothing unless it received exactly that many, so a transfer cut
///         off early, which the host sees only as the end of its input, is
///         refused rather than committed.
///     input: The new contents.
///     cancelled: Kills the transfer when set; the host's file is unchanged
///         unless the rename already happened.
///
/// Errors:
///     [`crate::Error::RemoteFile`] for a refusal, a conflict, a transfer
///     that was not `len` bytes ([`Error::Incomplete`]), access it cannot
///     keep ([`Error::Access`]), an in-place write refused in a folder
///     others can write ([`Error::SharedFolder`]), or a failed script, or a
///     target or platform
///     error from [`crate::Error`].
pub fn push_remote_file(
    file: RemoteFile<'_>,
    expect: Expect,
    access: Access<'_>,
    len: u64,
    input: impl Read + Send,
    cancelled: &AtomicBool,
) -> Result<()> {
    let script = match file.shell {
        RemoteShell::Posix => posix::push_script(file.path, expect, access, len),
        RemoteShell::PowerShell => powershell::push_script(file.path, expect, access, len),
    };
    let command = remote_command(file.target, file.shell, &script)?;
    Ok(push_with(command, input, cancelled)?)
}

fn remote_command(target: &str, shell: RemoteShell, script: &str) -> Result<Command> {
    match shell {
        RemoteShell::Posix => crate::ssh::script_command(target, script),
        RemoteShell::PowerShell => crate::ssh::powershell_script_command(target, script),
    }
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
        (Some(ACCESS_EXIT), _) => Error::Access,
        (Some(SHARED_FOLDER_EXIT), _) => Error::SharedFolder,
        (Some(SHORT_EXIT), _) => Error::Incomplete,
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
