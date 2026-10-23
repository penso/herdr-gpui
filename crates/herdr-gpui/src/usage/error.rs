//! Why reading a provider's plan usage failed.

use std::io;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("Usage command failed: {0}.")]
    Command(&'static str),
    #[error("Could not reach the usage service from this host.")]
    Connect,
    #[error("Saved sign-in cannot be sent as a header.")]
    Header(#[source] ureq::http::header::InvalidHeaderValue),
    /// The body may hold account details, so only the parser's category is kept.
    #[error("Usage response was not the expected JSON.")]
    Json(serde_json::error::Category),
    #[error("Reading this sign-in needs Keychain access, which macOS asks for.")]
    KeychainAccess,
    #[error("Keychain access was denied, so this sign-in cannot be read.")]
    KeychainDenied,
    #[error("curl is not installed on this host, so usage cannot be read.")]
    MissingCurl,
    #[error("Usage request mixes this machine's settings with the remote host's sign-in.")]
    MixedSecrets,
    #[error("Usage request failed or timed out.")]
    Network(#[source] ureq::Error),
    #[error("This account has no plan with usage limits to show.")]
    NoPlan,
    #[error("No sign-in found on this host. Set it up under [usage.providers] in the config.")]
    NotSignedIn,
    #[error("Could not {operation}.")]
    Process {
        operation: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("Usage is rate limited. Retrying later.")]
    RateLimited,
    #[error("Saved sign-in was rejected. Open the agent to sign in again.")]
    Rejected,
    #[error("Usage output exceeded the size limit.")]
    Size,
    #[error("Usage service returned HTTP {0}.")]
    Status(u16),
    #[error("Usage check timed out.")]
    Timeout,
    #[error("Could not reach this host over SSH to read usage.")]
    Unreachable,
    // keep-sorted end
}
