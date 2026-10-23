//! Why taking, listing, or restoring a checkpoint failed.

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("That checkpoint was taken on another branch. Check that branch out to restore it.")]
    Branch,
    #[error("A merge, rebase, cherry-pick, or revert is in progress. Finish or abort it first.")]
    Busy,
    #[error("No checkout of this branch was found on its host.")]
    Checkout,
    #[error("That checkpoint no longer exists.")]
    Missing,
    #[error("Checkpoint output was not understood.")]
    Output,
    #[error("Checkpoint Git commands failed")]
    Script(#[source] herdr_client::Error),
    #[error("Could not start the checkpoint worker")]
    Thread(#[source] io::Error),
    #[error("This checkout has no commit yet, so it has no checkpoints.")]
    Unborn,
    #[error("The checkpoint worker stopped.")]
    Worker,
    // keep-sorted end
}
