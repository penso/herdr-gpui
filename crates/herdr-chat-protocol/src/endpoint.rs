//! Where a chat program listens, and how a client learns of it.

use std::{
    ffi::OsString,
    fmt,
    path::{Path, PathBuf},
    str::FromStr,
};

use uuid::Uuid;

use crate::Error;

/// The Herdr pane metadata token whose value is the program's `EndpointId`.
pub const TOKEN: &str = "acp";

/// A program's socket name: 32 lowercase hexadecimal digits.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct EndpointId(String);

impl EndpointId {
    pub fn random() -> Self {
        Uuid::new_v4().into()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The id's 32 digits, as a session id that is already a UUID becomes the
/// endpoint of the view attached to it.
impl From<Uuid> for EndpointId {
    fn from(id: Uuid) -> Self {
        Self(id.simple().to_string())
    }
}

impl FromStr for EndpointId {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != 32
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(Error::InvalidEndpoint);
        }
        Ok(Self(value.to_owned()))
    }
}

impl fmt::Display for EndpointId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Where the chat with `id` listens: a socket in [`endpoint_dir`] on Unix,
/// a named pipe on Windows. `None` when no directory can be found.
pub fn endpoint_address(id: &EndpointId) -> Option<PathBuf> {
    if cfg!(windows) {
        return Some(PathBuf::from(format!(r"\\.\pipe\herdr-acp-{id}")));
    }
    endpoint_dir().map(|dir| endpoint_path(&dir, id))
}

/// The private directory chat sockets live in on Unix.
pub fn endpoint_dir() -> Option<PathBuf> {
    endpoint_dir_with(|name| std::env::var_os(name))
}

/// [`endpoint_dir`] with the environment supplied, for tests.
pub fn endpoint_dir_with(var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let var = |name| var(name).filter(|value| !value.is_empty());
    let root = var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| var("HOME").map(|home| PathBuf::from(home).join(".local").join("state")))?;
    Some(root.join("herdr").join("acp"))
}

pub fn endpoint_path(dir: &Path, id: &EndpointId) -> PathBuf {
    dir.join(format!("{id}.sock"))
}
