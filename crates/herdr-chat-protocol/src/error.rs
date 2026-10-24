use std::io;

use crate::{EntryId, PermissionId};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("permission request {0:?} is already pending")]
    DuplicatePermission(PermissionId),
    #[error("chat frame is empty")]
    EmptyFrame,
    #[error("pushed entry {id:?} does not follow the last entry {last:?}")]
    EntryOrder { id: EntryId, last: EntryId },
    #[error("chat frame of {len} bytes exceeds the limit")]
    FrameLimit { len: usize },
    #[error("not a chat endpoint id")]
    InvalidEndpoint,
    #[error("chat I/O failed")]
    Io(#[from] io::Error),
    #[error("chat message is not valid JSON for this protocol")]
    Json(#[from] serde_json::Error),
    #[error("the session has no modes")]
    NoModes,
    #[error("entry {0:?} has no text to append to")]
    NotText(EntryId),
    #[error("no entry {0:?}")]
    UnknownEntry(EntryId),
    #[error("no pending permission request {0:?}")]
    UnknownPermission(PermissionId),
}
