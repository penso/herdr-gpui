//! Persistent per-IP certificates: a LAN change leaves the Tailscale leaf
//! untouched. DNS names use a separate leaf; a custom certificate stays fixed.

use super::{Authority, Error, Names, Result, authority::LEAF_LIFETIME};
use rustls::{
    crypto::ring,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
    server::{ClientHello, ResolvesServerCert},
    sign::CertifiedKey,
};
use std::{
    collections::BTreeMap,
    fs,
    net::IpAddr,
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
    addresses: RwLock<BTreeMap<IpAddr, Current>>,
}

#[derive(Clone, Debug)]
struct Current {
    key: Arc<CertifiedKey>,
    origin: Origin,
}

#[derive(Clone, Debug)]
enum Origin {
    /// Issued by the daemon's CA for `names`, valid until `expires`.
    Issued { expires: SystemTime, names: Names },
    /// Supplied by the user; never reissued.
    Files,
}

impl Resolver {
    /// Loads the certificates saved for these addresses, issuing only those
    /// missing or near expiry. A valid shared legacy leaf seeds its covered
    /// addresses without changing the certificate phones already trust. Blocking.
    pub(crate) fn issue(authority: &Authority, names: Names, now: SystemTime) -> Result<Self> {
        // Migrate IPs before renewing the shared legacy leaf: an already-trusted
        // certificate remains the identity of every address it still covers.
        let addresses = address_keys(authority, &names, &BTreeMap::new(), now)?;
        let names = dns_names(&names);
        let stored = authority.stored(None).map(Current::from);
        let current = match stored {
            Some(stored) if !stored.needs_reissue(&names, now) => stored,
            _ => Current::issue(authority, names, now)?,
        };
        Ok(Self {
            current: RwLock::new(current),
            addresses: RwLock::new(addresses),
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
            addresses: RwLock::new(BTreeMap::new()),
        })
    }

    /// Adds or renews certificates for current addresses, independently. Retired
    /// addresses leave memory but keep their files for a later return. The live
    /// map is bounded by local interfaces; handshakes never generate certificates.
    /// User-supplied certificates are left alone. Blocking; call off the executor.
    pub(crate) fn refresh(
        &self,
        authority: &Authority,
        names: &Names,
        now: SystemTime,
    ) -> Result<bool> {
        if matches!(self.read().origin, Origin::Files) {
            return Ok(false);
        }
        let old = self
            .addresses
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        let next = address_keys(authority, names, &old, now)?;
        let changed = next.iter().any(|(ip, current)| {
            old.get(ip)
                .is_none_or(|old| !Arc::ptr_eq(&old.key, &current.key))
        });
        drop(old);
        let names = dns_names(names);
        let renew_dns = self.read().needs_reissue(&names, now);
        let dns = if renew_dns {
            Some(Current::issue(authority, names, now)?)
        } else {
            None
        };
        // Prepare and persist all replacements before publishing any of them.
        if let Some(dns) = dns {
            *self.current.write().unwrap_or_else(PoisonError::into_inner) = dns;
        }
        *self
            .addresses
            .write()
            .unwrap_or_else(PoisonError::into_inner) = next;
        Ok(changed || renew_dns)
    }

    /// The DNS certificate, or the user's fixed certificate.
    pub(crate) fn current(&self) -> Arc<CertifiedKey> {
        Arc::clone(&self.read().key)
    }

    pub(crate) fn at(self: &Arc<Self>, address: IpAddr) -> Arc<dyn ResolvesServerCert> {
        Arc::new(AddressResolver {
            resolver: Arc::clone(self),
            address: address.to_canonical(),
        })
    }

    pub(crate) fn current_for(&self, address: IpAddr) -> Option<Arc<CertifiedKey>> {
        let current = self.read();
        if matches!(current.origin, Origin::Files) {
            return Some(Arc::clone(&current.key));
        }
        drop(current);
        self.addresses
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&address.to_canonical())
            .map(|current| Arc::clone(&current.key))
    }

    fn read(&self) -> std::sync::RwLockReadGuard<'_, Current> {
        // A panic elsewhere cannot leave `Current` half-written: it is only
        // ever replaced whole.
        self.current.read().unwrap_or_else(PoisonError::into_inner)
    }
}

impl From<super::stored::Stored> for Current {
    fn from(stored: super::stored::Stored) -> Self {
        Self {
            key: stored.key,
            origin: Origin::Issued {
                expires: stored.expires,
                names: stored.names,
            },
        }
    }
}

fn dns_names(names: &Names) -> Names {
    Names {
        dns: names.dns.clone(),
        ips: Vec::new(),
    }
}

fn address_keys(
    authority: &Authority,
    names: &Names,
    old: &BTreeMap<IpAddr, Current>,
    now: SystemTime,
) -> Result<BTreeMap<IpAddr, Current>> {
    names
        .ips
        .iter()
        .map(|ip| {
            let ip = ip.to_canonical();
            let names = Names {
                ips: vec![ip],
                dns: Vec::new(),
            };
            if let Some(current) = old
                .get(&ip)
                .filter(|current| !current.needs_reissue(&names, now))
            {
                return Ok((ip, current.clone()));
            }
            if let Some(current) = authority
                .stored(Some(ip))
                .map(Current::from)
                .filter(|current| !current.needs_reissue(&names, now))
            {
                return Ok((ip, current));
            }
            if let Some(legacy) = authority
                .stored(None)
                .map(Current::from)
                .filter(|current| !current.needs_reissue(&names, now))
            {
                authority.migrate_leaf(ip)?;
                return Ok((ip, legacy));
            }
            let key = authority.issue_for(&names, now, Some(ip))?;
            let expires = now.checked_add(LEAF_LIFETIME).unwrap_or(now);
            Ok((
                ip,
                Current {
                    key,
                    origin: Origin::Issued { expires, names },
                },
            ))
        })
        .collect()
}

#[derive(Debug)]
struct AddressResolver {
    resolver: Arc<Resolver>,
    address: IpAddr,
}

impl ResolvesServerCert for AddressResolver {
    fn resolve(&self, hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        if hello.server_name().is_some() {
            Some(self.resolver.current())
        } else {
            self.resolver.current_for(self.address)
        }
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
                // An address the computer no longer has stays listed: the
                // CA only vouches for private ones, and keeping the leaf
                // spares phones that accepted it a new warning.
                !names.ips.iter().all(|ip| listed.ips.contains(ip))
                    || !names.dns.iter().all(|name| listed.dns.contains(name))
                    // `duration_since` fails once `expires` has passed.
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
