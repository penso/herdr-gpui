//! The conversation so far of a session this program continues. Claude and
//! Codex keep it in their own logs and send none of it when a session
//! opens, unlike an ACP agent's `session/load`, so without this a continued
//! chat would know everything and show nothing.
//!
//! Only the exact session's log is read, with the same bounded reader the
//! mirror uses; a log is data, and nothing in it is run.

mod error;

pub use error::{Error, Result};

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use sheprd_chat_protocol::Transcript;

use crate::{
    mirror::transcript::{Source, TranscriptAgent},
    observe::identity,
};

/// The history of `session`, read from `agent`'s config directory.
pub(crate) fn load(agent: TranscriptAgent, session: &str) -> Result<Transcript> {
    let (variable, default) = match agent {
        TranscriptAgent::Claude => ("CLAUDE_CONFIG_DIR", ".claude"),
        TranscriptAgent::Codex => ("CODEX_HOME", ".codex"),
    };
    let base = config_directory(
        std::env::var_os(variable),
        std::env::home_dir().as_deref(),
        default,
    )
    .ok_or(Error::NoHome)?;
    read(agent, session, &base)
}

/// The directory an agent keeps its logs in: its override, else `default`
/// in the home folder.
fn config_directory(
    custom: Option<OsString>,
    home: Option<&Path>,
    default: &str,
) -> Option<PathBuf> {
    custom
        .filter(|custom| !custom.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.map(|home| home.join(default)))
}

/// The history of `session` from the one log under `base` that holds it.
fn read(agent: TranscriptAgent, session: &str, base: &Path) -> Result<Transcript> {
    let candidates = match agent {
        TranscriptAgent::Claude => identity::claude_logs(base, session)?,
        TranscriptAgent::Codex => identity::codex_logs(base, session)?,
    };
    // A file named after the session must also say it is that session.
    let mut logs = candidates
        .into_iter()
        .filter(|path| identity::matches_session(path, agent, session).unwrap_or(false));
    let path = logs.next().ok_or(Error::NotFound)?;
    if logs.next().is_some() {
        return Err(Error::Ambiguous);
    }
    Source::new(path, agent).read()?.ok_or(Error::NotFound)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
