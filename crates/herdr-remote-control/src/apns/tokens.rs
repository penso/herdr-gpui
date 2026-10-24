//! The push tokens one device registers: its app's push-to-start token,
//! which starts an activity while the app is closed, and one token per
//! activity it shows, which updates and ends it. The device store keeps a
//! [`Registration`] as opaque JSON, as it keeps a Web Push subscription.
//!
//! `POST /api/apns` takes the same shape:
//!
//! ```json
//! { "push_to_start": "80a1…", "activities": [{ "pane": "w1-p3", "token": "5c3e…" }] }
//! ```

use super::{Error, Result};
use serde::{Deserialize, Serialize};

/// The most activities one device registers; iOS shows only a few at once.
pub(crate) const MAX_ACTIVITIES: usize = 32;
/// Bounds on a token's length in bytes. APNs device tokens are 32 bytes;
/// Live Activity tokens are longer, and Apple documents no fixed length.
const MIN_TOKEN: usize = 16;
const MAX_TOKEN: usize = 256;
const MAX_PANE: usize = 128;

/// An APNs token as lowercase hex, the form the request path takes.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub(crate) struct DeviceToken(String);

impl DeviceToken {
    pub(crate) fn parse(hex: &str) -> Result<Self> {
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

    pub(crate) fn as_str(&self) -> &str {
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

/// The token that updates the activity for the chat in `pane`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ActivityToken {
    pub(crate) pane: String,
    pub(crate) token: DeviceToken,
}

/// Everything one device registered.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Registration {
    #[serde(default)]
    pub(crate) activities: Vec<ActivityToken>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) push_to_start: Option<DeviceToken>,
}

impl Registration {
    /// Reads a `POST /api/apns` body, checking its bounds.
    pub(crate) fn parse(body: &[u8]) -> Result<Self> {
        let registration: Self = serde_json::from_slice(body)?;
        registration.check()?;
        Ok(registration)
    }

    /// Reads what the device store kept; anything unreadable is nothing.
    pub(crate) fn from_stored(value: Option<&serde_json::Value>) -> Self {
        value
            .and_then(|value| Self::deserialize(value).ok())
            .filter(|registration| registration.check().is_ok())
            .unwrap_or_default()
    }

    /// The JSON the device store keeps, or `None` when nothing is left.
    pub(crate) fn to_stored(&self) -> Result<Option<serde_json::Value>> {
        if self.is_empty() {
            return Ok(None);
        }
        Ok(Some(serde_json::to_value(self)?))
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.push_to_start.is_none() && self.activities.is_empty()
    }

    fn check(&self) -> Result<()> {
        if self.activities.len() > MAX_ACTIVITIES {
            return Err(Error::TooManyActivities {
                len: self.activities.len(),
            });
        }
        let valid_pane = |pane: &str| {
            (1..=MAX_PANE).contains(&pane.chars().count()) && !pane.chars().any(char::is_control)
        };
        if !self
            .activities
            .iter()
            .all(|activity| valid_pane(&activity.pane))
        {
            return Err(Error::Pane);
        }
        Ok(())
    }

    /// Applies what the app sent: a push-to-start token replaces the old one
    /// when present, and the activity list replaces the old list, since the
    /// app sends every activity it still shows. A later token for one pane
    /// wins over an earlier one.
    pub(crate) fn merge(&mut self, update: Self) {
        if update.push_to_start.is_some() {
            self.push_to_start = update.push_to_start;
        }
        self.activities.clear();
        for activity in update.activities.into_iter().rev() {
            if !self
                .activities
                .iter()
                .any(|kept| kept.pane == activity.pane)
            {
                self.activities.push(activity);
            }
        }
        self.activities.reverse();
    }

    /// The token for the chat in `pane`'s activity.
    pub(crate) fn activity(&self, pane: &str) -> Option<&DeviceToken> {
        self.activities
            .iter()
            .find(|activity| activity.pane == pane)
            .map(|activity| &activity.token)
    }

    /// Forgets `token` after APNs called it invalid; `true` when it was
    /// there.
    pub(crate) fn forget(&mut self, token: &DeviceToken) -> bool {
        let before = self.activities.len();
        self.activities.retain(|activity| &activity.token != token);
        let mut forgot = self.activities.len() != before;
        if self.push_to_start.as_ref() == Some(token) {
            self.push_to_start = None;
            forgot = true;
        }
        forgot
    }

    /// Forgets the activity of a chat whose turn ended.
    pub(crate) fn end(&mut self, pane: &str) -> bool {
        let before = self.activities.len();
        self.activities.retain(|activity| activity.pane != pane);
        self.activities.len() != before
    }
}
