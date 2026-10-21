//! Process inspection failures.

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Herdr's answer about the pane's process was not understood.")]
    Answer(#[source] serde_json::Error),
    #[error("Still ending the last processes. Try again in a moment.")]
    Busy,
    #[error("The herdr executable's path is not valid UTF-8.")]
    Executable,
    #[error("Herdr did not name a process for this pane.")]
    NoRoot,
    #[error("Could not ask Herdr which process the pane runs.")]
    Query(#[source] herdr_client::Error),
    #[error("The pane's process has exited.")]
    RootExited,
    #[error("The process list stopped updating. Reopen it to try again.")]
    Stopped,
    #[error("Could not start watching the pane's processes.")]
    Worker(#[source] io::Error),
    // keep-sorted end
}
