//! The daemon's certificate authority: created once, kept in the private
//! state directory, and used to issue server certificates.

use super::{
    Error, Names, Result,
    names::{self, LOCAL_DOMAINS, PRIVATE_NETWORKS},
};
use rcgen::{
    BasicConstraints, CertificateParams, CidrSubnet, DistinguishedName, DnType,
    ExtendedKeyUsagePurpose, GeneralSubtree, IsCa, Issuer, KeyPair, KeyUsagePurpose,
    NameConstraints, PKCS_ECDSA_P256_SHA256, SanType, string::Ia5String,
};
use rustls::{
    crypto::ring,
    pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, pem::PemObject},
    sign::CertifiedKey,
};
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime},
};
use time::OffsetDateTime;

/// Subdirectory of the state directory holding the CA.
const TLS_DIR: &str = "tls";
const CA_CERT_FILE: &str = "ca.pem";
const CA_KEY_FILE: &str = "ca-key.pem";
/// About ten years: the phone installs the CA once.
const CA_LIFETIME: Duration = Duration::from_secs(3653 * 24 * 60 * 60);
/// Well under the 398 days Apple accepts; the resolver renews it early.
pub(super) const LEAF_LIFETIME: Duration = Duration::from_secs(90 * 24 * 60 * 60);
/// Backdating covers a phone whose clock runs a little behind.
const BACKDATE: Duration = Duration::from_secs(60 * 60);
const LEAF_COMMON_NAME: &str = "Herdr Remote";

/// A P-256 certificate authority, name-constrained to private addresses and
/// local names. Its `Debug` output never shows the private key.
#[derive(Debug)]
pub(crate) struct Authority {
    ca_der: CertificateDer<'static>,
    ca_pem: String,
    fingerprint: String,
    issuer: Issuer<'static, KeyPair>,
}

impl Authority {
    /// Loads the CA from `state_dir/tls/`, creating it there first if it is
    /// missing. The directory is private (0700) and both files are 0600 on
    /// Unix; each is written to a temporary file and renamed into place.
    /// Blocking.
    pub(crate) fn load_or_create(state_dir: &Path) -> Result<Self> {
        let dir = state_dir.join(TLS_DIR);
        create_private_dir(&dir)?;
        let key_path = dir.join(CA_KEY_FILE);
        let cert_path = dir.join(CA_CERT_FILE);
        if let (Some(key_pem), Some(cert_pem)) =
            (read_optional(&key_path)?, read_optional(&cert_path)?)
        {
            return Self::from_pem(&key_pem, &key_path, cert_pem, &cert_path);
        }
        let host = names::host_name();
        let (authority, key_pem) = Self::create(host.as_deref(), SystemTime::now())?;
        // The key first: a CA certificate without its key is never loaded.
        write_private(&key_path, key_pem.as_bytes())?;
        write_private(&cert_path, authority.ca_pem.as_bytes())?;
        Ok(authority)
    }

    /// A new CA for `host`, valid from `now`, with its key as PEM.
    pub(super) fn create(host: Option<&str>, now: SystemTime) -> Result<(Self, String)> {
        let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).map_err(Error::Certificate)?;
        let key_pem = key.serialize_pem();
        let mut params = CertificateParams::default();
        params.distinguished_name = ca_name(host);
        params.is_ca = IsCa::Ca(BasicConstraints::Constrained(0));
        params.key_usages = vec![
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
            KeyUsagePurpose::DigitalSignature,
        ];
        // Chain-level EKU: whatever this CA signs is good for TLS servers only.
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        params.name_constraints = Some(name_constraints());
        (params.not_before, params.not_after) = validity(now, CA_LIFETIME);
        let certificate = params.self_signed(&key).map_err(Error::Certificate)?;
        let ca_der = certificate.der().clone();
        let authority = Self {
            ca_pem: certificate.pem(),
            fingerprint: fingerprint(&ca_der),
            ca_der,
            issuer: Issuer::new(params, key),
        };
        Ok((authority, key_pem))
    }

    fn from_pem(key_pem: &str, key_path: &Path, ca_pem: String, cert_path: &Path) -> Result<Self> {
        let key = KeyPair::from_pem(key_pem).map_err(|source| Error::InvalidAuthority {
            path: key_path.to_owned(),
            source,
        })?;
        let ca_der =
            CertificateDer::from_pem_slice(ca_pem.as_bytes()).map_err(|source| Error::Pem {
                path: cert_path.to_owned(),
                source,
            })?;
        let issuer =
            Issuer::from_ca_cert_der(&ca_der, key).map_err(|source| Error::InvalidAuthority {
                path: cert_path.to_owned(),
                source,
            })?;
        Ok(Self {
            ca_pem,
            fingerprint: fingerprint(&ca_der),
            ca_der,
            issuer,
        })
    }

    /// SHA-256 of the CA certificate, as uppercase colon-separated hex: what
    /// the pairing QR carries and both screens show.
    pub(crate) fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// The CA certificate as PEM, for an Android or desktop download.
    pub(crate) fn ca_pem(&self) -> &str {
        &self.ca_pem
    }

    /// The iOS configuration profile that installs this CA.
    /// The CA certificate as DER.
    #[cfg(test)]
    pub(crate) fn ca_der(&self) -> &[u8] {
        &self.ca_der
    }

    pub(crate) fn mobileconfig(&self) -> Vec<u8> {
        super::profile::mobileconfig(&self.ca_der, &self.fingerprint)
    }

    /// A new server certificate for `names`, valid from `now` for
    /// [`LEAF_LIFETIME`], with a fresh P-256 key. Its chain is the leaf then
    /// this CA.
    pub(crate) fn issue(&self, names: &Names, now: SystemTime) -> Result<Arc<CertifiedKey>> {
        let key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256).map_err(Error::Certificate)?;
        let mut params = CertificateParams::default();
        params.distinguished_name = DistinguishedName::new();
        params
            .distinguished_name
            .push(DnType::CommonName, LEAF_COMMON_NAME);
        params.is_ca = IsCa::ExplicitNoCa;
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
        params.use_authority_key_identifier_extension = true;
        params.subject_alt_names = names.ips.iter().copied().map(SanType::IpAddress).collect();
        for name in &names.dns {
            let name = Ia5String::try_from(name.as_str()).map_err(Error::Certificate)?;
            params.subject_alt_names.push(SanType::DnsName(name));
        }
        (params.not_before, params.not_after) = validity(now, LEAF_LIFETIME);
        let leaf = params
            .signed_by(&key, &self.issuer)
            .map_err(Error::Certificate)?;
        let key_der = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(key.serialize_der()));
        let signing_key = ring::default_provider()
            .key_provider
            .load_private_key(key_der)
            .map_err(Error::InvalidIdentity)?;
        Ok(Arc::new(CertifiedKey::new(
            vec![leaf.der().clone(), self.ca_der.clone()],
            signing_key,
        )))
    }
}

/// "Herdr Remote CA (HOSTNAME)", so the phone's certificate list says
/// which computer it trusts.
fn ca_name(host: Option<&str>) -> DistinguishedName {
    let common_name = match host.and_then(|host| host.split('.').next()) {
        Some(label) if !label.is_empty() => format!("Herdr Remote CA ({label})"),
        _ => "Herdr Remote CA".to_owned(),
    };
    let mut name = DistinguishedName::new();
    name.push(DnType::CommonName, common_name);
    name
}

fn name_constraints() -> NameConstraints {
    let addresses = PRIVATE_NETWORKS.iter().map(|(network, prefix)| {
        GeneralSubtree::IpAddress(CidrSubnet::from_addr_prefix(*network, *prefix))
    });
    let domains = LOCAL_DOMAINS
        .iter()
        .map(|domain| GeneralSubtree::DnsName((*domain).to_owned()));
    NameConstraints {
        permitted_subtrees: addresses.chain(domains).collect(),
        excluded_subtrees: Vec::new(),
    }
}

/// `not_before` and `not_after` for a certificate issued at `now`.
fn validity(now: SystemTime, lifetime: Duration) -> (OffsetDateTime, OffsetDateTime) {
    let start = now.checked_sub(BACKDATE).unwrap_or(now);
    let end = now.checked_add(lifetime).unwrap_or(now);
    (start.into(), end.into())
}

fn fingerprint(der: &[u8]) -> String {
    Sha256::digest(der)
        .iter()
        .map(|byte| format!("{byte:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

fn read_optional(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(Error::Read {
            path: path.to_owned(),
            source,
        }),
    }
}

fn create_private_dir(dir: &Path) -> Result<()> {
    let error = |source| Error::CreateDir {
        path: dir.to_owned(),
        source,
    };
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        builder.mode(0o700);
        builder.create(dir).map_err(error)?;
        // `mode` only applies to a directory this call created.
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700)).map_err(error)
    }
    #[cfg(not(unix))]
    builder.create(dir).map_err(error)
}

/// Writes `contents` to `path` through a temporary sibling, readable only
/// by this user on Unix, so a crash never leaves a half-written file.
fn write_private(path: &Path, contents: &[u8]) -> Result<()> {
    let temporary = temporary_path(path);
    let error = |path: &Path| {
        let path = path.to_owned();
        move |source| Error::Write { path, source }
    };
    let result = write_new(&temporary, contents).map_err(error(&temporary));
    let result = result.and_then(|()| fs::rename(&temporary, path).map_err(error(path)));
    if result.is_err() {
        // Best effort: the failure being reported matters more.
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn temporary_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_owned();
    name.push(format!(".tmp-{}", std::process::id()));
    path.with_file_name(name)
}

fn write_new(path: &Path, contents: &[u8]) -> io::Result<()> {
    use std::io::Write;
    // A leftover from a crashed run may have other permissions; start anew.
    match fs::remove_file(path) {
        Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(contents)?;
    file.sync_all()
}
