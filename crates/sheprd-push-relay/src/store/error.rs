#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("the relay holds as many revocations as it can")]
    Full,
    #[cfg(feature = "postgres")]
    #[error("could not read {}", path.display())]
    PostgresCa {
        path: std::path::PathBuf,
        #[source]
        source: rustls::pki_types::pem::Error,
    },
    #[cfg(feature = "postgres")]
    #[error("{} holds a certificate the relay cannot use", path.display())]
    PostgresCaInvalid {
        path: std::path::PathBuf,
        #[source]
        source: rustls::Error,
    },
    #[cfg(feature = "postgres")]
    #[error("the Postgres database failed")]
    Postgres(#[from] tokio_postgres::Error),
    #[error("the SQLite database failed")]
    Sqlite(#[from] rusqlite::Error),
    #[error("the database task did not finish")]
    Task(#[from] tokio::task::JoinError),
    // keep-sorted end
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
