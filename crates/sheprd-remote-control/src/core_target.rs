//! The desktop's core choice survives a service manager's different environment.

use sheprd_core_client::{Endpoint, Error, Result};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CoreTarget {
    // keep-sorted start
    #[default]
    Discover,
    Endpoint(Endpoint),
    InvalidSession(String),
    NoHome,
    // keep-sorted end
}

impl CoreTarget {
    /// Hex keeps control characters out of the service definition.
    pub(crate) fn encoded_session(session: &str) -> String {
        use std::fmt::Write as _;
        let mut encoded = String::new();
        for byte in session.bytes() {
            let _ = write!(encoded, "{byte:02x}");
        }
        encoded
    }

    #[cfg(feature = "server")]
    pub(crate) fn invalid_session(encoded: &str) -> Option<Self> {
        if !encoded.len().is_multiple_of(2) {
            return None;
        }
        let bytes = encoded
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok())
            .collect::<Option<Vec<_>>>()?;
        Some(Self::InvalidSession(String::from_utf8(bytes).ok()?))
    }

    pub(crate) fn resolved(result: Result<Endpoint>) -> Result<Self> {
        match result {
            Ok(endpoint) => Ok(Self::Endpoint(endpoint)),
            Err(Error::Session(session)) => Ok(Self::InvalidSession(session)),
            Err(Error::NoHome) => Ok(Self::NoHome),
            Err(error) => Err(error),
        }
    }

    #[cfg(feature = "server")]
    pub(crate) fn endpoint(&self) -> Result<Endpoint> {
        match self {
            Self::Discover => Endpoint::resolve(sheprd_client::login_env::var),
            Self::Endpoint(endpoint) => Ok(endpoint.clone()),
            Self::InvalidSession(session) => Err(Error::Session(session.clone())),
            Self::NoHome => Err(Error::NoHome),
        }
    }
}
