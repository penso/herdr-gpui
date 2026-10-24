//! The daemon's VAPID key (RFC 8292): one P-256 key pair, created once and
//! kept private in the state directory. Every push subscription a phone
//! makes is bound to its public half, so losing or replacing the key orphans
//! every subscription.

use super::{Error, Result};
use base64::Engine as _;
use p256::{
    SecretKey,
    ecdsa::{Signature, SigningKey, signature::Signer as _},
    elliptic_curve::zeroize::Zeroizing,
    pkcs8::LineEnding,
};
use serde::Serialize;
use std::{
    fs,
    io::{self, Write as _},
    path::{Path, PathBuf},
};

/// How long a token is valid; RFC 8292 allows at most a day.
pub(super) const TOKEN_LIFETIME_S: u64 = 12 * 60 * 60;
const _: () = assert!(TOKEN_LIFETIME_S <= 24 * 60 * 60);

/// The private key file, relative to the state directory.
pub(super) fn key_path(state_dir: &Path) -> PathBuf {
    state_dir.join("push").join("vapid.pem")
}

/// The sender identity every push is signed with.
#[derive(Debug)]
pub(crate) struct Vapid {
    key: SigningKey,
    public: String,
}

#[derive(Serialize)]
struct Claims<'a> {
    aud: &'a str,
    exp: u64,
    sub: &'a str,
}

impl Vapid {
    /// Loads the key from `state_dir`, creating and storing one the first
    /// time. Blocking file I/O.
    pub(crate) fn load_or_create(state_dir: &Path) -> Result<Self> {
        let path = key_path(state_dir);
        match load(&path) {
            Ok(vapid) => return Ok(vapid),
            Err(Error::KeyStore { source, .. }) if source.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let key = generate()?;
        let pem = key.to_sec1_pem(LineEnding::LF).map_err(Error::KeyEncode)?;
        match store_new(&path, pem.as_bytes()) {
            Ok(()) => Ok(Self::from(key)),
            // Another process stored its key first; theirs is the one.
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => load(&path),
            Err(source) => Err(Error::KeyStore { path, source }),
        }
    }

    /// The uncompressed public point in base64url, which the page passes to
    /// `PushManager.subscribe({applicationServerKey})`.
    pub(crate) fn public_key_b64url(&self) -> &str {
        &self.public
    }

    /// The `Authorization` header for a push to the service at `audience`
    /// (its origin, such as `https://fcm.googleapis.com`): `vapid t=JWT,k=KEY`
    /// with an ES256 token valid for twelve hours from `now_s`. `subject` is
    /// a `mailto:` or `https:` contact for the push service.
    pub(crate) fn authorization(
        &self,
        audience: &str,
        subject: &str,
        now_s: u64,
    ) -> Result<String> {
        check_subject(subject)?;
        let header = B64.encode(br#"{"typ":"JWT","alg":"ES256"}"#);
        let claims = serde_json::to_vec(&Claims {
            aud: audience,
            exp: now_s.saturating_add(TOKEN_LIFETIME_S),
            sub: subject,
        })?;
        let signing_input = format!("{header}.{}", B64.encode(claims));
        let signature: Signature = self
            .key
            .try_sign(signing_input.as_bytes())
            .map_err(Error::Sign)?;
        let signature = B64.encode(signature.to_bytes());
        Ok(format!(
            "vapid t={signing_input}.{signature},k={}",
            self.public
        ))
    }
}

impl From<SecretKey> for Vapid {
    fn from(secret: SecretKey) -> Self {
        let public = B64.encode(secret.public_key().to_sec1_bytes());
        Self {
            key: SigningKey::from(secret),
            public,
        }
    }
}

/// base64url without padding, as JWTs, VAPID, and `PushSubscription` use;
/// decoding also takes padded input.
pub(super) const B64: base64::engine::GeneralPurpose = base64::engine::GeneralPurpose::new(
    &base64::alphabet::URL_SAFE,
    base64::engine::GeneralPurposeConfig::new()
        .with_encode_padding(false)
        .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent),
);

pub(super) fn check_subject(subject: &str) -> Result<()> {
    let valid = ["mailto:", "https://"]
        .iter()
        .any(|scheme| subject.len() > scheme.len() && subject.starts_with(scheme));
    if valid { Ok(()) } else { Err(Error::Subject) }
}

/// A random P-256 secret key.
pub(super) fn generate() -> Result<SecretKey> {
    // A random 32-byte string is a valid scalar unless it is zero or at least
    // the group order, which happens with probability about 2^-32.
    let mut bytes = Zeroizing::new([0; 32]);
    loop {
        getrandom::fill(bytes.as_mut_slice()).map_err(Error::Random)?;
        if let Ok(key) = SecretKey::from_slice(bytes.as_slice()) {
            return Ok(key);
        }
    }
}

fn load(path: &Path) -> Result<Vapid> {
    let store = |source| Error::KeyStore {
        path: path.to_owned(),
        source,
    };
    let metadata = fs::symlink_metadata(path).map_err(store)?;
    if !is_private(&metadata) {
        return Err(Error::KeyNotPrivate {
            path: path.to_owned(),
        });
    }
    let pem = fs::read_to_string(path).map_err(store)?;
    let secret = SecretKey::from_sec1_pem(&pem).map_err(|source| Error::KeyDecode {
        path: path.to_owned(),
        source,
    })?;
    Ok(Vapid::from(secret))
}

#[cfg(unix)]
pub(crate) fn is_private(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    metadata.is_file() && metadata.mode() & 0o077 == 0
}

#[cfg(not(unix))]
pub(crate) fn is_private(metadata: &fs::Metadata) -> bool {
    metadata.is_file()
}

/// Writes `contents` to `path` only if nothing is there yet: into a private
/// temporary file first, then linked into place, which fails rather than
/// replacing a key another process stored meanwhile.
fn store_new(path: &Path, contents: &[u8]) -> io::Result<()> {
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::from(io::ErrorKind::InvalidInput))?;
    create_private_dir(directory)?;
    let mut suffix = [0; 8];
    getrandom::fill(&mut suffix).map_err(io::Error::other)?;
    let temporary = directory.join(format!(".vapid-{}.tmp", u64::from_le_bytes(suffix)));
    let written =
        write_private(&temporary, contents).and_then(|()| fs::hard_link(&temporary, path));
    let removed = fs::remove_file(&temporary);
    written?;
    removed
}

fn create_private_dir(directory: &Path) -> io::Result<()> {
    if let Some(parent) = directory.parent() {
        fs::create_dir_all(parent)?;
    }
    #[cfg(unix)]
    let builder = {
        let mut builder = fs::DirBuilder::new();
        std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
        builder
    };
    #[cfg(not(unix))]
    let builder = fs::DirBuilder::new();
    match builder.create(directory) {
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists && directory.is_dir() => Ok(()),
        result => result,
    }
}

fn write_private(path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path)?;
    file.write_all(contents)?;
    file.sync_all()
}
