//! APNs credentials: an Apple Developer token key (`.p8`, PKCS#8 PEM) and
//! which team, key, and app it belongs to. Nothing here creates them; without
//! `config.json` there is no sender.
//!
//! ```json
//! { "team_id": "ABCDE12345", "key_id": "XYZ9876543",
//!   "topic": "so.pen.herdr.remote", "sandbox": true }
//! ```

use crate::{Error, Host, Result};
use p256::{SecretKey, ecdsa::SigningKey, pkcs8::DecodePrivateKey as _};
use serde::Deserialize;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

/// The longest config or key file read; both are a few hundred bytes.
const MAX_FILE: u64 = 16 * 1024;
/// The longest bundle identifier the topic may name.
const MAX_TOPIC: usize = 155;

/// The directory holding the key and config, inside the state directory.
pub fn dir(state_dir: &Path) -> PathBuf {
    state_dir.join("apns")
}

pub(crate) fn config_path(state_dir: &Path) -> PathBuf {
    dir(state_dir).join("config.json")
}

pub(crate) fn key_path(state_dir: &Path) -> PathBuf {
    dir(state_dir).join("AuthKey.p8")
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    key_id: String,
    #[serde(default)]
    sandbox: bool,
    team_id: String,
    topic: String,
}

/// Loaded and checked APNs credentials.
#[derive(Debug)]
pub struct Config {
    pub host: Host,
    pub key: SigningKey,
    /// The key's ten-character identifier, the JWT's `kid`.
    pub key_id: String,
    /// The ten-character Apple Developer team, the JWT's `iss`.
    pub team_id: String,
    /// The app's bundle identifier, such as `so.pen.herdr.remote`.
    pub topic: String,
}

impl Config {
    /// Reads `STATE_DIR/apns/config.json` and the key beside it: `None`
    /// when there is no config. Blocking file I/O.
    pub fn load(state_dir: &Path) -> Result<Option<Self>> {
        let path = config_path(state_dir);
        let bytes = match read_bounded(&path) {
            Ok(bytes) => bytes,
            Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(source) => return Err(Error::ConfigRead { path, source }),
        };
        let file: ConfigFile = serde_json::from_slice(&bytes)
            .map_err(|source| Error::ConfigFormat { path, source })?;
        let key = load_key(&key_path(state_dir))?;
        Self::new(file, key).map(Some)
    }

    /// Credentials from their parts, with the same checks as a config file.
    pub fn from_parts(
        team_id: &str,
        key_id: &str,
        topic: &str,
        sandbox: bool,
        key: SigningKey,
    ) -> Result<Self> {
        Self::new(
            ConfigFile {
                key_id: key_id.to_owned(),
                sandbox,
                team_id: team_id.to_owned(),
                topic: topic.to_owned(),
            },
            key,
        )
    }

    fn new(file: ConfigFile, key: SigningKey) -> Result<Self> {
        if !apple_id(&file.team_id) {
            return Err(Error::ConfigField { field: "team_id" });
        }
        if !apple_id(&file.key_id) {
            return Err(Error::ConfigField { field: "key_id" });
        }
        if !bundle_id(&file.topic) {
            return Err(Error::ConfigField { field: "topic" });
        }
        Ok(Self {
            host: if file.sandbox {
                Host::Sandbox
            } else {
                Host::Production
            },
            key,
            key_id: file.key_id,
            team_id: file.team_id,
            topic: file.topic,
        })
    }

    /// The `apns-topic` of a Live Activity push for this app.
    pub fn live_activity_topic(&self) -> String {
        format!("{}.push-type.liveactivity", self.topic)
    }
}

/// Reads a PKCS#8 PEM P-256 key, refusing one other users can read.
pub(crate) fn load_key(path: &Path) -> Result<SigningKey> {
    load_secret_key(path).map(SigningKey::from)
}

/// Reads a PKCS#8 PEM P-256 key, refusing one other users can read.
pub fn load_secret_key(path: &Path) -> Result<SecretKey> {
    let read = |source| Error::KeyRead {
        path: path.to_owned(),
        source,
    };
    let metadata = fs::symlink_metadata(path).map_err(read)?;
    if !is_private(&metadata) {
        return Err(Error::KeyNotPrivate {
            path: path.to_owned(),
        });
    }
    let pem = String::from_utf8(read_bounded(path).map_err(read)?)
        .map_err(|_| read(io::Error::from(io::ErrorKind::InvalidData)))?;
    SecretKey::from_pkcs8_pem(&pem).map_err(|source| Error::KeyDecode {
        path: path.to_owned(),
        source,
    })
}

fn read_bounded(path: &Path) -> io::Result<Vec<u8>> {
    use std::io::Read as _;
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(MAX_FILE + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_FILE {
        return Err(io::Error::from(io::ErrorKind::FileTooLarge));
    }
    Ok(bytes)
}

/// Apple's team and key identifiers: ten uppercase letters or digits.
pub(crate) fn apple_id(text: &str) -> bool {
    text.len() == 10
        && text
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

/// A bundle identifier: dot-separated letters, digits, and hyphens.
pub(crate) fn bundle_id(text: &str) -> bool {
    (1..=MAX_TOPIC).contains(&text.len())
        && text.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

/// Whether `metadata` is a regular file only its owner can read.
#[cfg(unix)]
pub fn is_private(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    metadata.is_file() && metadata.mode() & 0o077 == 0
}

/// Whether `metadata` is a regular file; other platforms have no mode bits.
#[cfg(not(unix))]
pub fn is_private(metadata: &fs::Metadata) -> bool {
    metadata.is_file()
}
