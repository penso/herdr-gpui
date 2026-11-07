//! Why dropped or pasted paths could not be quoted for a pane.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Dropped paths must not contain control characters.")]
    Control,
    #[error("Dropped paths must not be empty.")]
    EmptyPath,
    #[error("Dropped paths must be UTF-8.")]
    Encoding,
    #[error("File drop exceeds 256 paths or 64 KiB of quoted text.")]
    Size,
    // keep-sorted end
}
