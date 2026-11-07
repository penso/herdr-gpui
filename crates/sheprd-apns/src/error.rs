use std::{io, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("could not read the APNs response")]
    Body(#[source] Box<dyn std::error::Error + Send + Sync>),
    #[error("{field} in the APNs config is not valid")]
    ConfigField { field: &'static str },
    #[error("{} is not a valid APNs config", path.display())]
    ConfigFormat {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("could not read the APNs config {}", path.display())]
    ConfigRead {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not connect to APNs")]
    Connect(#[source] io::Error),
    #[error("an APNs device token must be 16 to 256 bytes of hex")]
    DeviceToken,
    #[error("APNs host {host} is not one of Apple's push hosts")]
    Host { host: String },
    #[error("the APNs request failed")]
    Http(#[from] hyper::Error),
    #[error("could not encode the APNs provider token")]
    Json(#[from] serde_json::Error),
    #[error("could not read the APNs key {}", path.display())]
    KeyDecode {
        path: PathBuf,
        #[source]
        source: p256::pkcs8::Error,
    },
    #[error("the APNs key {} is open to other users", path.display())]
    KeyNotPrivate { path: PathBuf },
    #[error("could not read the APNs key {}", path.display())]
    KeyRead {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not open the sealed bytes")]
    Open,
    #[error("could not get random bytes to seal a notification")]
    Random(#[source] getrandom::Error),
    #[error("could not build the APNs request")]
    Request(#[from] hyper::http::Error),
    #[error("could not seal the notification")]
    Seal,
    #[error("could not sign the APNs provider token")]
    Sign(#[source] p256::ecdsa::Error),
    #[error("APNs did not answer in time")]
    Timeout,
    #[error("could not set up TLS for APNs")]
    Tls(#[source] rustls::Error),
    // keep-sorted end
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
