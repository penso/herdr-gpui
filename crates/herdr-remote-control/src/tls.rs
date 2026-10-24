//! TLS without DNS: the daemon's own certificate authority and the server
//! certificates it issues for the addresses it serves on.
//!
//! - [`Authority`] is a P-256 CA created once in the state directory and
//!   name-constrained to private addresses and local names, so a stolen key
//!   cannot vouch for public sites. The phone installs it once, from
//!   [`Authority::mobileconfig`] on iOS or [`Authority::ca_pem`] elsewhere.
//! - [`Names`] lists what this machine is reachable as right now: LAN and
//!   Tailscale addresses, loopback, `localhost`, and `HOSTNAME.local`.
//! - [`Resolver`] hands rustls the current leaf, and [`Resolver::refresh`]
//!   reissues it when the names change or it nears expiry, without a restart.
//!   A user-supplied certificate ([`Resolver::from_files`]) is never reissued.
//!
//! Everything here is blocking and runtime-free; the server decides when to
//! call it.

mod authority;
mod names;
mod profile;
mod resolver;

pub(crate) use authority::Authority;
pub(crate) use names::Names;
pub(crate) use resolver::Resolver;

use rustls::{ServerConfig, crypto::ring, version};
use std::{io, path::PathBuf, sync::Arc};

/// What the remote control server offers in ALPN: it speaks HTTP/1.1 only.
const ALPN_HTTP_1_1: &[u8] = b"http/1.1";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("could not create a certificate")]
    Certificate(#[source] rcgen::Error),
    #[error("could not create {}", path.display())]
    CreateDir {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("the certificate authority in {} is unreadable", path.display())]
    InvalidAuthority {
        path: PathBuf,
        #[source]
        source: rcgen::Error,
    },
    #[error("the certificate and key do not form a usable server identity")]
    InvalidIdentity(#[source] rustls::Error),
    #[error("{} holds no certificate", path.display())]
    NoCertificate { path: PathBuf },
    #[error("{} is not valid PEM", path.display())]
    Pem {
        path: PathBuf,
        #[source]
        source: rustls::pki_types::pem::Error,
    },
    #[error("could not read {}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("could not configure TLS")]
    ServerConfig(#[source] rustls::Error),
    #[error("could not write {}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    // keep-sorted end
}

pub(crate) type Result<T> = std::result::Result<T, Error>;

/// The server's TLS configuration: ring, TLS 1.2 and 1.3, no client
/// certificates (phones prove themselves with their device key instead),
/// HTTP/1.1, and the certificate `resolver` picks for each handshake.
///
/// Fails only if ring lacks a cipher suite for these versions, which it
/// does not; the `Result` keeps that out of a panic.
pub(crate) fn server_config(resolver: Arc<Resolver>) -> Result<ServerConfig> {
    let mut config = ServerConfig::builder_with_provider(Arc::new(ring::default_provider()))
        .with_protocol_versions(&[&version::TLS13, &version::TLS12])
        .map_err(Error::ServerConfig)?
        .with_no_client_auth()
        .with_cert_resolver(resolver);
    config.alpn_protocols = vec![ALPN_HTTP_1_1.to_vec()];
    Ok(config)
}

#[cfg(test)]
mod tests;
