use std::io;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[cfg(feature = "server")]
    #[error(transparent)]
    Apns(#[from] crate::apns::Error),
    #[error(transparent)]
    App(#[from] crate::app::Error),
    #[cfg(feature = "server")]
    #[error(transparent)]
    Auth(#[from] crate::auth::Error),
    #[error(transparent)]
    Control(#[from] crate::control::Error),
    #[cfg(feature = "server")]
    #[error(transparent)]
    Hub(#[from] crate::hub::Error),
    #[error("remote control I/O failed")]
    Io(#[from] io::Error),
    #[cfg(feature = "server")]
    #[error(transparent)]
    Push(#[from] crate::push::Error),
    #[cfg(feature = "server")]
    #[error(transparent)]
    Tls(#[from] crate::tls::Error),
    // keep-sorted end
}
