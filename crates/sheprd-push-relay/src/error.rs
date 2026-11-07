use std::{ffi::OsString, io, net::SocketAddr, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error(transparent)]
    Apns(#[from] sheprd_apns::Error),
    #[error("{argument:?} is not a valid argument")]
    Argument { argument: OsString },
    #[error("could not listen on {address}")]
    Bind {
        address: SocketAddr,
        #[source]
        source: io::Error,
    },
    #[error("{} has no APNs config", path.display())]
    NoApnsConfig { path: PathBuf },
    #[cfg(not(feature = "postgres"))]
    #[error("this relay was built without Postgres")]
    NoPostgres,
    #[error("{} is open to other users", path.display())]
    NotPrivate { path: PathBuf },
    #[error("could not start the runtime")]
    Runtime(#[source] io::Error),
    #[error("could not read {}", path.display())]
    SecretRead {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("{} must hold at least 32 bytes as hex", path.display())]
    SecretSize { path: PathBuf },
    #[error("the server failed")]
    Serve(#[source] io::Error),
    #[error(transparent)]
    Store(#[from] crate::store::Error),
    // keep-sorted end
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
