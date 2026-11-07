use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("This computer has no home folder for new worktrees")]
    NoHome,
    #[error("That folder's repository has no name to file a worktree under")]
    NoRepositoryName,
    #[error("Git refused the new worktree: {0}")]
    Refused(String),
    #[error("Git could not be started")]
    Start(#[source] io::Error),
    // keep-sorted end
}
