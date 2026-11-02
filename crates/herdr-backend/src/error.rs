pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("a terminal needs at least one column and one row")]
    EmptySize,
    #[error("a terminal ID cannot be empty")]
    EmptyTerminalId,
    #[error("a terminal ID must be short and free of control characters")]
    InvalidTerminalId,
    #[error("a terminal of {cols}x{rows} cells is larger than the app shows")]
    LargeSize { cols: u16, rows: u16 },
    // keep-sorted end
}
