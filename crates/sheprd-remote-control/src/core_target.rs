//! The desktop's core choice survives a service manager's different environment.

use sheprd_core_client::{Endpoint, Result};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum CoreTarget {
    // keep-sorted start
    #[default]
    Discover,
    Endpoint(Endpoint),
    // keep-sorted end
}

impl CoreTarget {
    /// A desktop that cannot name its core leaves it to the service, whose
    /// core source reports the failure while Herdr 1 and chats keep working.
    pub(crate) fn resolved(result: Result<Endpoint>) -> Self {
        result.map_or(Self::Discover, Self::Endpoint)
    }

    #[cfg(feature = "server")]
    pub(crate) fn endpoint(&self) -> Result<Endpoint> {
        match self {
            Self::Discover => Endpoint::resolve(sheprd_client::login_env::var),
            Self::Endpoint(endpoint) => Ok(endpoint.clone()),
        }
    }
}
