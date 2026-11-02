//! Why VS Code did not run on an SSH host, from the exit status of the
//! script that starts it there (`serve.sh`).

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Could not download VS Code's command line on the host.")]
    Download,
    #[error("The host has neither curl nor wget to download VS Code's command line.")]
    NoDownloader,
    #[error("Could not prepare VS Code's folder in the host's home folder.")]
    Setup,
    #[error("SSH could not reach the host, or could not forward VS Code's port.")]
    Ssh,
    #[error("Could not unpack VS Code's command line on the host.")]
    Unpack,
    #[error("VS Code's command line is not made for this host's system or processor.")]
    Unsupported,
    // keep-sorted end
}
