//! Picks the server certificate for each handshake and swaps it when the
//! machine's names change.

use super::{Authority, Error, Names, Result, authority::LEAF_LIFETIME};
use rustls::{
    crypto::ring,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
    server::{ClientHello, ResolvesServerCert},
    sign::CertifiedKey,
};
use std::{
    fs,
    path::Path,
    sync::{Arc, PoisonError, RwLock},
    time::{Duration, SystemTime},
};

/// How long before expiry an issued leaf is replaced.
const RENEW_BEFORE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// The certificate the server presents. Handshakes read it; only
/// [`Resolver::refresh`] replaces it.
#[derive(Debug)]
pub(crate) struct Resolver {
    current: RwLock<Current>,
}

#[derive(Debug)]
struct Current {
    key: Arc<CertifiedKey>,
    origin: Origin,
}

#[derive(Debug)]
enum Origin {
    /// Issued by the daemon's CA for `names`, valid until `expires`.
    Issued { expires: SystemTime, names: Names },
    /// Supplied by the user; never reissued.
    Files,
}

impl Resolver {
    /// A resolver presenting a leaf `authority` issues for `names` at `now`.
    pub(crate) fn issue(authority: &Authority, names: Names, now: SystemTime) -> Result<Self> {
        let current = Current::issue(authority, names, now)?;
        Ok(Self {
            current: RwLock::new(current),
        })
    }

    /// A resolver presenting the user's own certificate: `cert` is a PEM
    /// chain, leaf first, and `key` a PEM PKCS#8, SEC1, or PKCS#1 private
    /// key matching the leaf. Blocking.
    pub(crate) fn from_files(cert: &Path, key: &Path) -> Result<Self> {
        let chain = read(cert)?;
        let chain = CertificateDer::pem_slice_iter(&chain)
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|source| Error::Pem {
                path: cert.to_owned(),
                source,
            })?;
        if chain.is_empty() {
            return Err(Error::NoCertificate {
                path: cert.to_owned(),
            });
        }
        let key_der = PrivateKeyDer::from_pem_slice(&read(key)?).map_err(|source| Error::Pem {
            path: key.to_owned(),
            source,
        })?;
        let certified = CertifiedKey::from_der(chain, key_der, &ring::default_provider())
            .map_err(Error::InvalidIdentity)?;
        Ok(Self {
            current: RwLock::new(Current {
                key: Arc::new(certified),
                origin: Origin::Files,
            }),
        })
    }

    /// Reissues the leaf when `names` differ from the ones it lists or it
    /// expires within a week of `now`, and reports whether it did. A
    /// user-supplied certificate is left alone. Blocking for the key
    /// generation only; handshakes keep the old leaf until the swap.
    pub(crate) fn refresh(
        &self,
        authority: &Authority,
        names: &Names,
        now: SystemTime,
    ) -> Result<bool> {
        if !self.read().needs_reissue(names, now) {
            return Ok(false);
        }
        let current = Current::issue(authority, names.clone(), now)?;
        *self.current.write().unwrap_or_else(PoisonError::into_inner) = current;
        Ok(true)
    }

    /// The certificate presented now.
    pub(crate) fn current(&self) -> Arc<CertifiedKey> {
        Arc::clone(&self.read().key)
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, Current> {
        // A panic elsewhere cannot leave `Current` half-written: it is only
        // ever replaced whole.
        self.current.read().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Current {
    fn issue(authority: &Authority, names: Names, now: SystemTime) -> Result<Self> {
        let key = authority.issue(&names, now)?;
        let expires = now.checked_add(LEAF_LIFETIME).unwrap_or(now);
        Ok(Self {
            key,
            origin: Origin::Issued { expires, names },
        })
    }

    fn needs_reissue(&self, names: &Names, now: SystemTime) -> bool {
        match &self.origin {
            Origin::Files => false,
            Origin::Issued {
                expires,
                names: listed,
            } => {
                // `duration_since` fails once `expires` has passed.
                listed != names
                    || !expires
                        .duration_since(now)
                        .is_ok_and(|left| left >= RENEW_BEFORE)
            }
        }
    }
}

fn read(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).map_err(|source| Error::Read {
        path: path.to_owned(),
        source,
    })
}

impl ResolvesServerCert for Resolver {
    fn resolve(&self, _client_hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(self.current())
    }
}
