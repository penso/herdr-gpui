#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Use a loopback HTTP px0 workbench address, without credentials, query or fragment")]
    Address,
    #[error("px0 is not installed; install it on your login shell's PATH, or set px0.url")]
    Missing,
    #[error("Cannot locate the app's state directory")]
    StateDirectory,
    #[error("Cannot read px0 discovery information: {0}")]
    Discovery(#[source] std::io::Error),
    #[error("Cannot start px0: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("Cannot start the px0 worker: {0}")]
    Worker(#[source] std::io::Error),
    #[error("Cannot reach the px0 workbench: {0}")]
    Http(#[source] ureq::Error),
    #[error("Cannot read the px0 response: {0}")]
    Read(#[source] std::io::Error),
    #[error("Invalid px0 workbench response: {0}")]
    Json(#[source] serde_json::Error),
    #[error("This address did not answer as a px0 workbench")]
    NotWorkbench,
    #[error("px0 did not publish its workbench address in time")]
    Timeout,
    #[error("px0 stopped before its workbench was ready")]
    Exited,
    #[error("Cannot check the px0 process: {0}")]
    Process(#[source] std::io::Error),
    #[error("The previous px0 worker did not finish stopping: {0}")]
    Shutdown(#[source] std::sync::mpsc::RecvTimeoutError),
}
