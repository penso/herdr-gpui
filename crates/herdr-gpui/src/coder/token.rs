//! The saved Coder sign-in: access and refresh tokens, when the access token
//! expires, and which deployment and OAuth client issued them. A record from
//! another deployment or client is never sent anywhere; it reads as signed out.

use super::{Error, Result, Settings};
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use zeroize::Zeroizing;

pub(crate) const LIMIT: usize = 64 * 1024;
/// Renew this long before expiry, so a connection never starts with a token
/// that lapses during the handshake.
const RENEW_BEFORE: Duration = Duration::from_secs(5 * 60);

pub(crate) fn valid(token: &str) -> bool {
    !token.is_empty() && token.len() <= 4096 && token.bytes().all(|b| b.is_ascii_graphic())
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Credential {
    version: u8,
    pub(crate) deployment: String,
    pub(crate) client_id: String,
    pub(crate) access_token: SecretString,
    pub(crate) refresh_token: Option<SecretString>,
    #[serde(default)]
    pub(crate) expires_at: Option<u64>,
}

impl Credential {
    pub(crate) fn new(
        settings: &Settings,
        access_token: SecretString,
        refresh_token: Option<SecretString>,
        expires_at: Option<u64>,
    ) -> Result<Self> {
        let credential = Self {
            version: 1,
            deployment: settings.base.clone(),
            client_id: settings.client_id.clone(),
            access_token,
            refresh_token,
            expires_at,
        };
        credential.validate()?;
        Ok(credential)
    }

    fn validate(&self) -> Result<()> {
        if self.version != 1
            || self.deployment.is_empty()
            || self.deployment.len() > 2048
            || self.client_id.is_empty()
            || self.client_id.len() > 256
            || !valid(self.access_token.expose_secret())
            || self
                .refresh_token
                .as_ref()
                .is_some_and(|token| !valid(token.expose_secret()))
        {
            return Err(Error::Token);
        }
        Ok(())
    }

    /// Whether this record was issued for the configured deployment and client.
    pub(crate) fn issued_for(&self, settings: &Settings) -> bool {
        self.deployment == settings.base && self.client_id == settings.client_id
    }

    pub(crate) fn renewal_due(&self, now: SystemTime) -> bool {
        self.refresh_token.is_some()
            && self.expires_at.is_some_and(|expires| {
                now.checked_add(RENEW_BEFORE)
                    .and_then(|soon| soon.duration_since(UNIX_EPOCH).ok())
                    .is_none_or(|soon| expires <= soon.as_secs())
            })
    }

    pub(crate) fn decode(value: &SecretString) -> Result<Self> {
        let text = value.expose_secret();
        if text.len() > LIMIT {
            return Err(Error::Token);
        }
        let credential: Self = serde_json::from_str(text).map_err(Error::json)?;
        credential.validate()?;
        Ok(credential)
    }

    pub(crate) fn encode(&self) -> Result<SecretString> {
        self.validate()?;
        #[derive(Serialize)]
        struct Record<'a> {
            version: u8,
            deployment: &'a str,
            client_id: &'a str,
            access_token: &'a str,
            refresh_token: Option<&'a str>,
            #[serde(skip_serializing_if = "Option::is_none")]
            expires_at: Option<u64>,
        }
        // Serialize into a preallocated buffer that is wiped when dropped.
        let mut bytes = Zeroizing::new(Vec::with_capacity(LIMIT));
        serde_json::to_writer(
            &mut *bytes,
            &Record {
                version: self.version,
                deployment: &self.deployment,
                client_id: &self.client_id,
                access_token: self.access_token.expose_secret(),
                refresh_token: self.refresh_token.as_ref().map(ExposeSecret::expose_secret),
                expires_at: self.expires_at,
            },
        )
        .map_err(Error::json)?;
        let text = std::str::from_utf8(&bytes).map_err(|_| Error::Token)?;
        Ok(text.into())
    }
}

/// Absolute expiry from a relative lifetime, `None` when absent or overflowing.
pub(crate) fn expiry(seconds: Option<u64>, now: SystemTime) -> Option<u64> {
    now.duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs()
        .checked_add(seconds?)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::coder::tests::settings;

    fn credential(expires_at: Option<u64>) -> Credential {
        Credential::new(
            &settings(),
            "access-fixture".into(),
            Some("refresh-fixture".into()),
            expires_at,
        )
        .unwrap()
    }

    #[test]
    fn record_roundtrips_and_is_redacted() {
        let value = credential(Some(123));
        let restored = Credential::decode(&value.encode().unwrap()).unwrap();
        assert_eq!(restored.access_token.expose_secret(), "access-fixture");
        assert_eq!(
            restored.refresh_token.unwrap().expose_secret(),
            "refresh-fixture"
        );
        assert_eq!(restored.expires_at, Some(123));
        assert!(restored.deployment == settings().base);
        let debug = format!("{value:?}");
        assert!(!debug.contains("access-fixture") && !debug.contains("refresh-fixture"));
    }

    #[test]
    fn renewal_is_due_shortly_before_expiry_and_only_with_a_refresh_token() {
        let now = UNIX_EPOCH + Duration::from_secs(1_000_000);
        let at = |seconds: u64| Some(1_000_000 + seconds);
        assert!(!credential(at(3600)).renewal_due(now));
        assert!(credential(at(300)).renewal_due(now));
        assert!(credential(at(0)).renewal_due(now));
        assert!(!credential(None).renewal_due(now));
        let mut access_only = credential(at(0));
        access_only.refresh_token = None;
        assert!(!access_only.renewal_due(now));
        assert_eq!(expiry(Some(60), now), at(60));
        assert_eq!(expiry(Some(u64::MAX), now), None);
        assert_eq!(expiry(None, now), None);
    }

    #[test]
    fn records_from_another_deployment_or_client_are_not_used() {
        let value = credential(None);
        assert!(value.issued_for(&settings()));
        let mut other = settings();
        other.base = "https://other.example.com".into();
        assert!(!value.issued_for(&other));
        let mut other = settings();
        other.client_id = "other-client".into();
        assert!(!value.issued_for(&other));
    }

    #[test]
    fn malformed_records_are_rejected_without_echoing_secrets() {
        for record in [
            r#"{"version":2,"deployment":"d","client_id":"c","access_token":"secret","refresh_token":null}"#,
            r#"{"version":1,"deployment":"","client_id":"c","access_token":"secret","refresh_token":null}"#,
            r#"{"version":1,"deployment":"d","client_id":"c","access_token":"","refresh_token":null}"#,
            r#"{"version":1,"deployment":"d","client_id":"c","access_token":"se cret","refresh_token":null}"#,
            r#"{"version":1,"deployment":"d","client_id":"c","access_token":"secret","refresh_token":"","extra":1}"#,
            "secret",
        ] {
            let error = Credential::decode(&record.into()).unwrap_err();
            assert!(!format!("{error:?}").contains("secret"));
            assert!(!error.to_string().contains("secret"));
        }
        assert!(Credential::decode(&"x".repeat(LIMIT + 1).into()).is_err());
    }
}
