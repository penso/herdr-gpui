//! A per-machine SSH command: a trusted local executable with fixed leading
//! arguments, such as `tailscale ssh` or `/path/to/ssh -F /path/to/config`.
//!
//! Commands are parsed with shell-style word quoting (via `shlex`) and run
//! directly with [`std::process::Command`], never through a shell: no
//! variable, command, or glob expansion happens, and a quoted word reaches the
//! child exactly as typed. `Default` is the plain `ssh` executable; an empty
//! or whitespace-only command resets to that default.

use crate::{Error, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::sync::Arc;

/// The longest accepted command, in bytes, and the most words it may hold.
const MAX_BYTES: usize = 4096;
const MAX_WORDS: usize = 64;

/// A per-machine SSH command. The executable is the first word; any remaining
/// words are its fixed arguments. Two spellings that quote the same words are
/// one command: equality, hashing, and `as_str` all use the canonical form.
///
/// The parsed words live behind an `Arc`, so carrying a `SshCommand` between
/// workers and cache keys clones only a reference, not every word. The plain
/// `ssh` default is a unit case that allocates nothing.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SshCommand {
    inner: Inner,
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum Inner {
    /// The plain `ssh` executable: `as_str` is `"ssh"`.
    Default,
    /// A custom executable and its fixed arguments, in canonical quoted form.
    Custom(Arc<Invocation>),
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Invocation {
    serialized: String,
    words: Vec<String>,
}

/// An SSH endpoint plus the command that reaches it. `target` is the validated
/// `[user@]host` catalog entry; `command` defaults to plain `ssh`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SshTarget {
    pub target: String,
    pub command: SshCommand,
}

impl Default for SshCommand {
    fn default() -> Self {
        Self {
            inner: Inner::Default,
        }
    }
}

impl SshCommand {
    /// The saved representation: the canonical, shell-quoted command string.
    pub fn as_str(&self) -> &str {
        match &self.inner {
            Inner::Default => "ssh",
            Inner::Custom(invocation) => &invocation.serialized,
        }
    }

    /// Whether this is the plain `ssh` default.
    pub fn is_default(&self) -> bool {
        matches!(self.inner, Inner::Default)
    }

    /// The executable word, run directly (never through a shell).
    #[cfg(unix)]
    pub(crate) fn executable(&self) -> &str {
        match &self.inner {
            Inner::Default => "ssh",
            Inner::Custom(invocation) => &invocation.words[0],
        }
    }

    /// The fixed arguments that follow the executable.
    #[cfg(unix)]
    pub(crate) fn arguments(&self) -> &[String] {
        match &self.inner {
            Inner::Default => &[],
            Inner::Custom(invocation) => &invocation.words[1..],
        }
    }

    /// A `Command` for the executable, with the application's environment
    /// applied and no fixed arguments yet.
    #[cfg(unix)]
    pub(crate) fn empty_command(&self) -> std::process::Command {
        crate::environment::command(self.executable())
    }

    /// Whether this invokes `tailscale ssh`, which takes the destination as
    /// its first subcommand argument rather than after OpenSSH options. The
    /// executable may be a full path, and global `tailscale` flags (including
    /// a separate `--socket`/`--timeout` value) may sit before `ssh`.
    #[cfg(unix)]
    pub(crate) fn is_tailscale_ssh(&self) -> bool {
        self.tailscale_arguments().is_some()
    }

    #[cfg(unix)]
    pub(crate) fn tailscale_arguments(&self) -> Option<(&[String], &[String])> {
        let Inner::Custom(invocation) = &self.inner else {
            return None;
        };
        if std::path::Path::new(&invocation.words[0])
            .file_name()?
            .to_str()?
            != "tailscale"
        {
            return None;
        }
        let arguments = &invocation.words[1..];
        let index = tailscale_subcommand(arguments)?;
        (arguments[index] == "ssh").then(|| arguments.split_at(index + 1))
    }

    fn parse(text: &str) -> Result<Self> {
        if text.len() > MAX_BYTES {
            return Err(Error::InvalidSshCommand {
                reason: "command exceeds 4096 bytes",
            });
        }
        if text.trim().is_empty() {
            return Ok(Self::default());
        }
        if text.chars().any(char::is_control) {
            return Err(Error::InvalidSshCommand {
                reason: "command contains a control character",
            });
        }
        let text = text.trim();
        let words = shlex::split(text).ok_or(Error::InvalidSshCommand {
            reason: "incomplete quoting",
        })?;
        if words.is_empty() {
            return Err(Error::InvalidSshCommand {
                reason: "command has no executable",
            });
        }
        if words.len() > MAX_WORDS {
            return Err(Error::InvalidSshCommand {
                reason: "command has more than 64 words",
            });
        }
        if words[0].is_empty() {
            return Err(Error::InvalidSshCommand {
                reason: "command executable is empty",
            });
        }
        if words[0].starts_with('-') {
            return Err(Error::InvalidSshCommand {
                reason: "command executable must not be an option",
            });
        }
        // The bare default is a unit case; anything else is a custom command.
        if words.len() == 1 && words[0] == "ssh" {
            return Ok(Self::default());
        }
        let serialized = shlex::try_join(words.iter().map(String::as_str)).map_err(|_| {
            Error::InvalidSshCommand {
                reason: "command cannot be quoted",
            }
        })?;
        Ok(Self {
            inner: Inner::Custom(Arc::new(Invocation { serialized, words })),
        })
    }
}

impl fmt::Debug for SshCommand {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("SshCommand").field(&self.as_str()).finish()
    }
}

impl TryFrom<&str> for SshCommand {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        Self::parse(value)
    }
}

impl TryFrom<String> for SshCommand {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        Self::parse(&value)
    }
}

impl Serialize for SshCommand {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for SshCommand {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::try_from(text).map_err(serde::de::Error::custom)
    }
}

impl From<String> for SshTarget {
    fn from(target: String) -> Self {
        Self {
            target,
            command: SshCommand::default(),
        }
    }
}

impl From<&str> for SshTarget {
    fn from(target: &str) -> Self {
        Self {
            target: target.to_owned(),
            command: SshCommand::default(),
        }
    }
}

impl fmt::Display for SshTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.target)
    }
}

/// The first non-global-flag argument of a `tailscale` invocation: `ssh` for
/// `tailscale ssh`, after skipping `--flag` and `--flag=value` global flags.
/// `--socket` and `--timeout` take a separate value, so that value is skipped
/// too.
#[cfg(unix)]
fn tailscale_subcommand(arguments: &[String]) -> Option<usize> {
    let mut args = arguments.iter().enumerate();
    while let Some((index, arg)) = args.next() {
        if let Some(flag) = arg.strip_prefix("--") {
            if !flag.contains('=') && matches!(flag, "socket" | "timeout") {
                args.next();
            }
            continue;
        }
        return Some(index);
    }
    None
}

#[cfg(test)]
mod tests;
