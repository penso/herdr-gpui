//! Issued server certificates and their keys, kept beside the CA as private
//! atomic PEM files. IP filenames encode the address, never user-controlled
//! paths. `leaf.pem` holds the DNS leaf and may be an older shared certificate
//! which the resolver migrates to per-IP files without changing its bytes.

use super::{Names, Result};
use rustls::{
    crypto::ring,
    pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject},
    sign::CertifiedKey,
};
use std::{
    net::IpAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};
use x509_parser::{certificate::X509Certificate, extensions::ParsedExtension};

/// The leaf certificate then its private key, as PEM, in one 0600 file.
pub(super) fn path(dir: &Path, address: Option<IpAddr>) -> PathBuf {
    dir.join(match address.map(|ip| ip.to_canonical()) {
        None => "leaf.pem".into(),
        Some(IpAddr::V4(ip)) => format!("leaf-ip4-{:08x}.pem", u32::from(ip)),
        Some(IpAddr::V6(ip)) => format!("leaf-ip6-{:032x}.pem", u128::from(ip)),
    })
}

/// A leaf read back from the state directory.
pub(super) struct Stored {
    pub(super) key: Arc<CertifiedKey>,
    pub(super) names: Names,
    pub(super) expires: SystemTime,
}

/// Saves the leaf `cert_pem` and its `key_pem` in `dir`. Blocking.
pub(super) fn save(
    dir: &Path,
    address: Option<IpAddr>,
    cert_pem: &str,
    key_pem: &str,
) -> Result<()> {
    super::authority::write_private(
        &path(dir, address),
        format!("{cert_pem}{key_pem}").as_bytes(),
    )
}

/// The leaf saved in `dir`, when there is one this CA (`ca_der`) issued and
/// its key matches; anything else is ignored and replaced. Blocking.
pub(super) fn load(
    dir: &Path,
    address: Option<IpAddr>,
    ca_der: &CertificateDer<'_>,
) -> Option<Stored> {
    let pem = std::fs::read(path(dir, address)).ok()?;
    let leaf = CertificateDer::pem_slice_iter(&pem).next()?.ok()?;
    let key_der = PrivateKeyDer::from_pem_slice(&pem).ok()?;
    let (_, ca) = x509_parser::parse_x509_certificate(ca_der).ok()?;
    let (_, parsed) = x509_parser::parse_x509_certificate(&leaf).ok()?;
    if !issued_by(&parsed, &ca) {
        return None;
    }
    let expires = u64::try_from(parsed.validity().not_after.timestamp())
        .ok()
        .and_then(|seconds| SystemTime::UNIX_EPOCH.checked_add(Duration::from_secs(seconds)))?;
    let names = Names::from_certificate(&leaf);
    // `from_der` checks that the key is the certificate's.
    let key = CertifiedKey::from_der(
        vec![leaf.clone(), ca_der.clone().into_owned()],
        key_der,
        &ring::default_provider(),
    )
    .ok()?;
    Some(Stored {
        key: Arc::new(key),
        names,
        expires,
    })
}

/// Whether `leaf` names `ca` as its issuer, by name and by key identifier.
/// The file is private to this user, so this guards against a leaf left
/// from a replaced CA, not against forgery.
fn issued_by(leaf: &X509Certificate<'_>, ca: &X509Certificate<'_>) -> bool {
    let subject_key =
        ca.extensions()
            .iter()
            .find_map(|extension| match extension.parsed_extension() {
                ParsedExtension::SubjectKeyIdentifier(id) => Some(id.0),
                _ => None,
            });
    let authority_key =
        leaf.extensions()
            .iter()
            .find_map(|extension| match extension.parsed_extension() {
                ParsedExtension::AuthorityKeyIdentifier(id) => {
                    id.key_identifier.as_ref().map(|id| id.0)
                }
                _ => None,
            });
    leaf.issuer().as_raw() == ca.subject().as_raw()
        && subject_key.is_some()
        && subject_key == authority_key
}
