//! The provider token APNs authenticates the daemon by: an ES256 JWT with
//! the key's id in its header and the team and issue time in its claims.
//! APNs refuses one older than an hour and throttles one renewed more often
//! than every twenty minutes, so it is reused for fifty.

use super::{Error, Result};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use p256::ecdsa::{Signature, SigningKey, signature::Signer as _};
use serde::Serialize;
use std::sync::{Mutex, PoisonError};

/// How long a token is reused before a new one is signed.
pub(crate) const REFRESH_S: u64 = 50 * 60;

#[derive(Serialize)]
struct Header<'a> {
    alg: &'static str,
    kid: &'a str,
}

#[derive(Serialize)]
struct Claims<'a> {
    iat: u64,
    iss: &'a str,
}

/// A JWT signed with `key` for `team_id`'s key `key_id`, issued at `iat`
/// (seconds since the Unix epoch).
pub(crate) fn sign(key: &SigningKey, team_id: &str, key_id: &str, iat: u64) -> Result<String> {
    let header = serde_json::to_vec(&Header {
        alg: "ES256",
        kid: key_id,
    })?;
    let claims = serde_json::to_vec(&Claims { iat, iss: team_id })?;
    let signing_input = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(header),
        URL_SAFE_NO_PAD.encode(claims)
    );
    // JWS ES256 is the P1363 form, r || s, which `to_bytes` gives.
    let signature: Signature = key
        .try_sign(signing_input.as_bytes())
        .map_err(Error::Sign)?;
    Ok(format!(
        "{signing_input}.{}",
        URL_SAFE_NO_PAD.encode(signature.to_bytes())
    ))
}

/// The current provider token, signed again when it is fifty minutes old or
/// APNs called it expired.
#[derive(Debug)]
pub(crate) struct ProviderToken {
    /// The token and when it was issued.
    cached: Mutex<Option<(u64, String)>>,
    key: SigningKey,
    key_id: String,
    team_id: String,
}

impl ProviderToken {
    pub(crate) fn new(key: SigningKey, team_id: String, key_id: String) -> Self {
        Self {
            cached: Mutex::new(None),
            key,
            key_id,
            team_id,
        }
    }

    /// The token to send at `now_s`.
    pub(crate) fn current(&self, now_s: u64) -> Result<String> {
        let mut cached = self.cached.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some((issued, token)) = cached.as_ref()
            && now_s >= *issued
            && now_s - issued < REFRESH_S
        {
            return Ok(token.clone());
        }
        let token = sign(&self.key, &self.team_id, &self.key_id, now_s)?;
        *cached = Some((now_s, token.clone()));
        Ok(token)
    }

    /// Forgets the token after APNs refused it, so the next push signs anew.
    pub(crate) fn invalidate(&self) {
        *self.cached.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }
}
