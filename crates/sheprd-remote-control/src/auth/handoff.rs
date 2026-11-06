//! Moving a paired phone's access into its Home Screen app. iOS gives an
//! installed web app its own storage, so the key saved while pairing in
//! the browser never reaches it. The paired browser mints a single-use
//! token; the installed app redeems it with its own key, which replaces
//! the browser's on the same device.
//!
//! Only each token's digest is kept, one per device: minting again voids
//! the device's previous token. The digests are saved in the state
//! directory, so an icon added before the daemon restarts still opens
//! paired; a digest cannot be turned back into its token.

use super::{
    Error, Result, SECRET_LEN, constant_time_eq, decode_fixed, devices::write_private, encode,
    random_bytes, sha256,
};
use crate::control::DeviceId;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

/// How long a minted token can be redeemed. The installed app's start
/// address keeps it, so it must outlast the gap between adding the app to
/// the Home Screen and first opening it; the browser reuses one token
/// until it nears expiry rather than voiding the one an icon holds.
pub(crate) const HANDOFF_TTL_MS: u64 = 24 * 60 * 60_000;

const FILE_NAME: &str = "handoffs.json";

/// A token the browser gives its Home Screen app.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Handoff {
    pub(crate) token: String,
    pub(crate) expires_ms: u64,
}

#[derive(Debug, Default)]
pub(crate) struct Handoffs {
    open: HashMap<DeviceId, ([u8; 32], u64)>,
    /// Where the digests are saved; none keeps them in memory only.
    path: Option<PathBuf>,
}

#[derive(Deserialize, Serialize)]
struct Saved {
    device: DeviceId,
    digest: String,
    expires_ms: u64,
}

impl Handoffs {
    /// The digests saved in `state_dir`. A missing or unreadable file holds
    /// none: the worst case is an icon that has to be added again.
    /// Blocking.
    pub(crate) fn load(state_dir: &Path) -> Self {
        let path = state_dir.join(FILE_NAME);
        let saved: Vec<Saved> = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        let open = saved
            .into_iter()
            .filter_map(|saved| {
                let digest = decode_fixed::<32>(&saved.digest).ok()?;
                Some((saved.device, (digest, saved.expires_ms)))
            })
            .collect();
        Self {
            open,
            path: Some(path),
        }
    }

    pub(crate) fn mint(&mut self, device: &DeviceId, now_ms: u64) -> Result<Handoff> {
        self.open.retain(|_, (_, expires)| *expires > now_ms);
        let secret = random_bytes::<SECRET_LEN>()?;
        let token = encode(&secret);
        let expires_ms = now_ms + HANDOFF_TTL_MS;
        self.open
            .insert(device.clone(), (sha256(&[token.as_bytes()]), expires_ms));
        self.save()?;
        Ok(Handoff { token, expires_ms })
    }

    /// The device `token` was minted for, using the token up; `None` when it
    /// is unknown, used, or expired. The use is saved before the device is
    /// returned, so a restart cannot make the token valid again.
    pub(crate) fn redeem(&mut self, token: &str, now_ms: u64) -> Result<Option<DeviceId>> {
        let digest = sha256(&[token.as_bytes()]);
        let Some(device) = self
            .open
            .iter()
            .find(|(_, (stored, expires))| *expires > now_ms && constant_time_eq(stored, &digest))
            .map(|(device, _)| device.clone())
        else {
            return Ok(None);
        };
        let entry = self.open.remove(&device);
        if let Err(error) = self.save() {
            if let Some(entry) = entry {
                self.open.insert(device, entry);
            }
            return Err(error);
        }
        Ok(Some(device))
    }

    /// Forgets a revoked device's token.
    pub(crate) fn forget(&mut self, device: &DeviceId) -> Result<()> {
        if self.open.remove(device).is_some() {
            self.save()?;
        }
        Ok(())
    }

    fn save(&self) -> Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let saved: Vec<Saved> = self
            .open
            .iter()
            .map(|(device, (digest, expires_ms))| Saved {
                device: device.clone(),
                digest: encode(digest),
                expires_ms: *expires_ms,
            })
            .collect();
        let content = serde_json::to_vec(&saved).map_err(Error::Encode)?;
        write_private(path, &content)
    }
}
