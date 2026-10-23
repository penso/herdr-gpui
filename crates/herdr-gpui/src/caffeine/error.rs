//! Why the machine could not be kept awake.

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Could not keep the machine awake: {0}")]
    KeepAwake(#[source] keepawake::Error),
    #[error("No agent is working, so there is nothing to stay awake for")]
    NoWorkingAgents,
    #[error("Could not start the keep-awake thread")]
    Thread(#[source] io::Error),
    #[error("The keep-awake thread stopped; the display may sleep")]
    WorkerStopped,
    // keep-sorted end
}
