use std::{io, path::PathBuf};

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not find the launcher's own path")]
    CurrentExe(#[source] io::Error),
    #[error("the launcher path {} has no parent directory", launcher.display())]
    NoParent { launcher: PathBuf },
    #[error("could not start {}", gui.display())]
    Spawn {
        gui: PathBuf,
        #[source]
        source: io::Error,
    },
}
