use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("A machine takes {min} to {max} CPUs on this Mac")]
    Cpus { min: u16, max: u16 },
    #[error("{}: {source}", path.display())]
    Delete {
        path: PathBuf,
        source: std::io::Error,
    },
    /// What Virtualization said, as its message: an `NSError` cannot leave
    /// the main thread.
    #[error("Virtualization refused the machine: {0}")]
    Framework(String),
    #[error("This machine was made for hardware this Mac does not have")]
    Hardware,
    #[error("A display is `fit` or a size such as 1920x1200")]
    InvalidDisplay,
    #[error(
        "A machine name takes letters, digits, '-', and '_', and starts with a letter or digit"
    )]
    InvalidName,
    #[error("A machine takes {min} to {max} GB of memory on this Mac")]
    Memory { min: u16, max: u16 },
    #[error("No state directory to keep machines in")]
    NoStateDir,
    #[error("No machine is installed at {}", .0.display())]
    NotInstalled(PathBuf),
    #[error("{}: {source}", path.display())]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("This Mac cannot run virtual Macs")]
    Unsupported,
    #[error("{}: {source}", path.display())]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
