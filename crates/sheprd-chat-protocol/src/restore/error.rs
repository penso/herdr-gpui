use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("a restore record names an agent this program does not know")]
    Agent,
    #[error("a daemon boot id is empty, too long, or has control characters")]
    Boot,
    #[error("a restore record's endpoint is not a chat endpoint id")]
    Endpoint,
    #[error("could not read or write a restore record")]
    Io(#[from] io::Error),
    #[error("a restore record is not valid JSON")]
    Json(#[from] serde_json::Error),
    #[error("a restore record's pane id is empty, too long, or has control characters")]
    Pane,
    #[error("a restore record's folder or socket is not an absolute path")]
    Path,
    #[error("a restore record's session id is not plain enough to type into a shell")]
    Session,
    #[error("a restore record exceeds {} bytes", super::MAX_RECORD_SIZE)]
    Size,
    #[error("a restore record has version {0}")]
    Version(u32),
    // keep-sorted end
}
