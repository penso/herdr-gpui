//! Why a job's log could not be read.

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("GitHub did not say where the job's log is.")]
    Location,
    #[error("The job's log could not be read.")]
    Read(#[source] std::io::Error),
    // keep-sorted end
}
