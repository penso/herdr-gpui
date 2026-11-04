#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("no private folder holds pasted images")]
    Folder,
    #[error("could not save the pasted image")]
    Write(#[source] std::io::Error),
    // keep-sorted end
}
