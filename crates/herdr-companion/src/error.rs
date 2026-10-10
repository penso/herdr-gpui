use std::io;

pub type Result<T> = std::result::Result<T, Error>;

/// Why an inbound HTTP request was refused before routing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HttpError {
    #[error("the connection closed before a complete request arrived")]
    Incomplete,
    #[error("the request line is malformed")]
    RequestLine,
    #[error("a header line is malformed")]
    Header,
    #[error("the request head exceeds its size limit")]
    HeadTooLarge,
    #[error("the request body exceeds its size limit")]
    BodyTooLarge,
    #[error("a request with a body must declare its Content-Length")]
    MissingContentLength,
    #[error("Content-Length is not a single decimal length")]
    ContentLength,
    #[error("transfer encodings are not accepted")]
    TransferEncoding,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("companion I/O failed: {0}")]
    Io(#[from] io::Error),
    #[error("invalid HTTP request: {0}")]
    Http(#[from] HttpError),
    #[error("invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("no pending request {0}")]
    UnknownRequest(u64),
    #[error("too many requests are already waiting for a decision")]
    PendingFull,
    #[error("request {0} is not a question, so it cannot take answers")]
    NotAQuestion(u64),
    #[error("a denial needs a message for the agent")]
    EmptyDenyMessage,
    #[error("a prompt must not be empty")]
    EmptyPrompt,
    #[error("no Herdr API socket is configured")]
    NoHerdrSocket,
    /// `code` and `message` are the daemon's own bounded diagnostic fields.
    #[error("Herdr refused the request ({code}): {message}")]
    Herdr { code: String, message: String },
    #[error("Herdr closed the API connection without replying")]
    HerdrClosed,
    #[error("Herdr's reply exceeds its size limit")]
    HerdrReplyTooLarge,
    #[error("Herdr's reply is missing an expected field")]
    HerdrReplyShape,
    #[error("send between 1 and 16 keys")]
    KeyCount,
    /// The rejected key name, echoed back to the client that sent it.
    #[error("key `{0}` is not allowed")]
    KeyNotAllowed(String),
    #[error("no transcript is known for that session")]
    NoTranscript,
    #[error("the pairing link does not fit in a QR code: {0}")]
    Qr(#[from] qrcode::types::QrError),
}
