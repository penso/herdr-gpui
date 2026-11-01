//! Phones' requests to be paired with the other hosts Herdr UI connects to
//! (`control::hosts`). What the desktop allowed and what each host answered
//! is kept on the device (`DeviceRecord::links`). The requests in flight
//! live here, in memory: a phone whose request is lost to a restart asks
//! again. Devices revoked here that hosts still hold are kept in
//! `host-revokes.json` until Herdr UI has revoked them there.

use super::{DeviceRecord, Devices, PublicKey, Result, device_name, devices::write_private};
use crate::control::{
    DeviceId,
    hosts::{
        Enrolled, Host, HostId, HostLink, HostOutcome, HostPairing, HostRevoke, HostState,
        MAX_HOSTS, MAX_REASON,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

const REVOKES_FILE: &str = "host-revokes.json";
/// Requests in flight at once, across every phone.
const MAX_ASKS: usize = 256;
/// Revocations kept for hosts that cannot be reached.
const MAX_REVOKES: usize = 1024;
/// How long Herdr UI may take to pair a host it took, asking the user
/// included, before the phone hears it failed.
pub(crate) const TAKEN_TTL_MS: u64 = 10 * 60 * 1000;
/// How long a request waits for Herdr UI to take it.
pub(crate) const QUEUED_TTL_MS: u64 = 24 * 60 * 60 * 1000;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    // keep-sorted start
    #[error("this host is already being paired")]
    Busy,
    #[error("a host name or id is not valid")]
    InvalidHost,
    #[error("the host's answer is not valid")]
    InvalidOutcome,
    #[error("no pairing with that host is in progress")]
    NotPairing,
    #[error("too many hosts are being paired; try again shortly")]
    TooMany,
    #[error("Herdr UI on this computer does not connect to that host")]
    UnknownHost,
    // keep-sorted end
}

/// A host the desktop allowed for a device, and what that host answered.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) struct Link {
    pub(crate) host: HostId,
    /// The host's name when it was allowed, shown once Herdr UI no longer
    /// lists it.
    pub(crate) name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) paired: Option<Enrolled>,
}

#[derive(Debug)]
struct Ask {
    name: String,
    public_key: PublicKey,
    confirm: bool,
    replace: Option<DeviceId>,
    stage: Stage,
    since_ms: u64,
}

#[derive(Debug, PartialEq, Eq)]
enum Stage {
    Queued,
    Taken,
    Failed(String),
}

#[derive(Debug)]
pub(crate) struct Links {
    /// The hosts Herdr UI last named.
    hosts: Vec<Host>,
    asks: HashMap<(DeviceId, HostId), Ask>,
    revokes: Vec<HostRevoke>,
    revokes_path: PathBuf,
}

impl Links {
    /// Reads the revocations still owed to hosts. A missing or unreadable
    /// file is none: a device it named stays on its host until it expires
    /// there. Blocking.
    pub(crate) fn load(state_dir: &Path) -> Self {
        let revokes_path = state_dir.join(REVOKES_FILE);
        let revokes = fs::read(&revokes_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            hosts: Vec::new(),
            asks: HashMap::new(),
            revokes,
            revokes_path,
        }
    }

    /// Replaces the hosts Herdr UI connects to.
    pub(crate) fn set_hosts(&mut self, mut hosts: Vec<Host>) -> Result<()> {
        if hosts.len() > MAX_HOSTS || !hosts.iter().all(Host::valid) {
            return Err(Error::InvalidHost.into());
        }
        let mut seen = std::collections::HashSet::new();
        hosts.retain(|host| seen.insert(host.id.clone()));
        self.hosts = hosts;
        Ok(())
    }

    /// What the desktop's Allow records for the hosts it ticked; ids Herdr
    /// UI never named are dropped.
    pub(crate) fn allowed(&self, ids: &[HostId]) -> Vec<Link> {
        self.hosts
            .iter()
            .filter(|host| ids.contains(&host.id))
            .map(|host| Link {
                host: host.id.clone(),
                name: host.name.clone(),
                paired: None,
            })
            .collect()
    }

    /// Every host as `device` sees it: the ones Herdr UI names, then the
    /// ones it was paired with that Herdr UI no longer names.
    pub(crate) fn view(&mut self, device: &DeviceRecord, now_ms: u64) -> Vec<HostLink> {
        self.refresh(now_ms);
        let known = self.hosts.iter().map(|host| (&host.id, &host.name));
        let gone = device
            .links
            .iter()
            .filter(|link| !self.hosts.iter().any(|host| host.id == link.host))
            .map(|link| (&link.host, &link.name));
        known
            .chain(gone)
            .map(|(host, name)| HostLink {
                host: host.clone(),
                name: name.clone(),
                state: self.state(device, host),
            })
            .collect()
    }

    fn state(&self, device: &DeviceRecord, host: &HostId) -> HostState {
        if let Some(ask) = self.asks.get(&(device.id.clone(), host.clone())) {
            return match &ask.stage {
                Stage::Failed(reason) => HostState::Failed {
                    reason: reason.clone(),
                },
                _ if ask.confirm => HostState::Asking,
                Stage::Queued | Stage::Taken => HostState::Pending,
            };
        }
        match device.links.iter().find(|link| &link.host == host) {
            Some(Link {
                paired: Some(enrolled),
                ..
            }) => HostState::Paired(enrolled.clone()),
            Some(_) => HostState::Allowed,
            None => HostState::Offered,
        }
    }

    /// The phone asks to be paired with `host` with a key it made for it.
    pub(crate) fn ask(
        &mut self,
        device: &DeviceRecord,
        host: &HostId,
        public_key: &str,
        now_ms: u64,
    ) -> Result<HostState> {
        self.refresh(now_ms);
        let public_key = PublicKey::parse(public_key)?;
        let link = device.links.iter().find(|link| &link.host == host);
        if link.is_none() && !self.hosts.iter().any(|known| &known.id == host) {
            return Err(Error::UnknownHost.into());
        }
        let key = (device.id.clone(), host.clone());
        match self.asks.get(&key) {
            Some(Ask {
                stage: Stage::Taken,
                ..
            }) => return Err(Error::Busy.into()),
            Some(_) => {}
            None if self.asks.len() >= MAX_ASKS => return Err(Error::TooMany.into()),
            None => {}
        }
        self.asks.insert(
            key,
            Ask {
                name: device_name(&device.name)?,
                public_key,
                confirm: link.is_none(),
                replace: link
                    .and_then(|link| link.paired.as_ref())
                    .map(|paired| paired.device.clone()),
                stage: Stage::Queued,
                since_ms: now_ms,
            },
        );
        Ok(self.state(device, host))
    }

    /// Hands Herdr UI the requests it has not taken yet.
    pub(crate) fn take(&mut self, now_ms: u64) -> Vec<HostPairing> {
        self.refresh(now_ms);
        self.asks
            .iter_mut()
            .filter(|(_, ask)| ask.stage == Stage::Queued)
            .map(|((device, host), ask)| {
                ask.stage = Stage::Taken;
                ask.since_ms = now_ms;
                HostPairing {
                    device: device.clone(),
                    host: host.clone(),
                    name: ask.name.clone(),
                    public_key: String::from(ask.public_key.clone()),
                    confirm: ask.confirm,
                    replace: ask.replace.clone(),
                }
            })
            .collect()
    }

    /// Records how Herdr UI's attempt ended. A host the phone is paired
    /// with now is kept on the device. With `public_key`, only the request
    /// for that key takes it: an attempt that outlasted the phone's retry
    /// must not answer the retry, whose key the phone now holds.
    pub(crate) fn finish(
        &mut self,
        devices: &mut Devices,
        device: &DeviceId,
        host: &HostId,
        public_key: Option<&str>,
        outcome: HostOutcome,
        now_ms: u64,
    ) -> Result<()> {
        self.refresh(now_ms);
        let key = (device.clone(), host.clone());
        let attempt = public_key.map(PublicKey::parse).transpose()?;
        let Some(ask) = self.asks.get_mut(&key).filter(|ask| {
            ask.stage == Stage::Taken && attempt.as_ref().is_none_or(|key| key == &ask.public_key)
        }) else {
            // A host that took the phone after its request ended (the phone
            // was revoked, the request timed out, or the phone asked again
            // with a new key) holds a device nobody will use: it is owed a
            // revocation.
            if let HostOutcome::Paired(enrolled) = outcome
                && enrolled.valid()
            {
                self.owe([HostRevoke {
                    host: host.clone(),
                    device: enrolled.device,
                }])?;
            }
            return Err(Error::NotPairing.into());
        };
        match outcome {
            HostOutcome::Paired(enrolled) => {
                if !enrolled.valid() {
                    return Err(Error::InvalidOutcome.into());
                }
                let name = self
                    .hosts
                    .iter()
                    .find(|known| &known.id == host)
                    .map(|known| known.name.clone())
                    .or_else(|| {
                        devices.get(device).and_then(|record| {
                            record
                                .links
                                .iter()
                                .find(|link| &link.host == host)
                                .map(|link| link.name.clone())
                        })
                    })
                    .unwrap_or_else(|| enrolled.computer.clone());
                devices.set_link(
                    device,
                    Link {
                        host: host.clone(),
                        name,
                        paired: Some(enrolled),
                    },
                )?;
                self.asks.remove(&key);
            }
            HostOutcome::Failed { reason } => {
                ask.stage = Stage::Failed(reason.chars().take(MAX_REASON).collect());
                ask.since_ms = now_ms;
            }
            HostOutcome::Denied => {
                self.asks.remove(&key);
            }
        }
        Ok(())
    }

    /// A device revoked here: its requests end, and every host it was
    /// paired with is owed a revocation. Blocking.
    pub(crate) fn forget(&mut self, device: &DeviceRecord) -> Result<()> {
        self.asks.retain(|(id, _), _| id != &device.id);
        self.owe(device.links.iter().filter_map(|link| {
            link.paired.as_ref().map(|paired| HostRevoke {
                host: link.host.clone(),
                device: paired.device.clone(),
            })
        }))
    }

    /// Adds revocations hosts are owed. Blocking when any is new. A failed
    /// write leaves them out, so the caller's retry adds and saves them
    /// again instead of finding them in memory only.
    fn owe(&mut self, owed: impl IntoIterator<Item = HostRevoke>) -> Result<()> {
        let mut revokes = self.revokes.clone();
        for revoke in owed {
            if !revokes.contains(&revoke) {
                revokes.push(revoke);
            }
        }
        if revokes.len() == self.revokes.len() {
            return Ok(());
        }
        // The oldest go first when hosts stay out of reach for long.
        let excess = revokes.len().saturating_sub(MAX_REVOKES);
        revokes.drain(..excess);
        self.replace_revokes(revokes)
    }

    pub(crate) fn revokes(&self) -> Vec<HostRevoke> {
        self.revokes.clone()
    }

    /// A host no longer holds the device. Blocking.
    pub(crate) fn revoked(&mut self, revoke: &HostRevoke) -> Result<()> {
        let mut revokes = self.revokes.clone();
        revokes.retain(|owed| owed != revoke);
        if revokes.len() == self.revokes.len() {
            return Ok(());
        }
        self.replace_revokes(revokes)
    }

    /// Writes `revokes`, then keeps them: memory never holds what the file
    /// does not.
    fn replace_revokes(&mut self, revokes: Vec<HostRevoke>) -> Result<()> {
        let content = serde_json::to_vec_pretty(&revokes).map_err(super::Error::Encode)?;
        write_private(&self.revokes_path, &content)?;
        self.revokes = revokes;
        Ok(())
    }

    /// Fails requests Herdr UI took and never finished, and drops ones it
    /// never took and failures nobody retried.
    fn refresh(&mut self, now_ms: u64) {
        self.asks.retain(|_, ask| {
            ask.stage == Stage::Taken || now_ms.saturating_sub(ask.since_ms) < QUEUED_TTL_MS
        });
        for ask in self.asks.values_mut() {
            if ask.stage == Stage::Taken && now_ms.saturating_sub(ask.since_ms) >= TAKEN_TTL_MS {
                ask.stage = Stage::Failed("Herdr UI did not finish pairing this host".to_owned());
                ask.since_ms = now_ms;
            }
        }
    }
}
