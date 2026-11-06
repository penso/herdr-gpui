//! A push token, checked before it reaches a request path.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};

/// Bounds on a token's length in bytes. APNs device tokens are 32 bytes;
/// Live Activity tokens are longer, and Apple documents no fixed length.
const MIN_TOKEN: usize = 16;
const MAX_TOKEN: usize = 256;

/// An APNs token as lowercase hex, the form the request path takes.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct DeviceToken(String);

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
        Ok(Self(hex.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
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
        token.0
    }
}
