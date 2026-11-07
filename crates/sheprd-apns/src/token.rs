//! A push token, checked before it reaches a request path. It reaches this
//! user's phone, so it is a secret: `Debug` hides it, and reading it takes
//! [`ExposeSecret`], which a search finds.

use crate::{Error, Result};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

/// Bounds on a token's length in bytes. APNs device tokens are 32 bytes;
/// Live Activity tokens are longer, and Apple documents no fixed length.
const MIN_TOKEN: usize = 16;
const MAX_TOKEN: usize = 256;

/// An APNs token as lowercase hex, the form the request path takes.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DeviceToken(SecretString);

impl DeviceToken {
    pub fn parse(hex: &str) -> Result<Self> {
        let hex = hex.trim();
        let bytes = hex.len() / 2;
        let valid = hex.len().is_multiple_of(2)
            && (MIN_TOKEN..=MAX_TOKEN).contains(&bytes)
            && hex.bytes().all(|byte| byte.is_ascii_hexdigit());
        if !valid {
            return Err(Error::DeviceToken);
        }
        Ok(Self(hex.to_ascii_lowercase().into()))
    }
}

impl ExposeSecret<str> for DeviceToken {
    fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}

impl PartialEq for DeviceToken {
    fn eq(&self, other: &Self) -> bool {
        self.expose_secret() == other.expose_secret()
    }
}

impl Eq for DeviceToken {}

impl Hash for DeviceToken {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.expose_secret().hash(state);
    }
}

impl TryFrom<String> for DeviceToken {
    type Error = Error;

    fn try_from(hex: String) -> Result<Self> {
        Self::parse(&hex)
    }
}

impl From<DeviceToken> for String {
    fn from(token: DeviceToken) -> Self {
        token.expose_secret().to_owned()
    }
}
