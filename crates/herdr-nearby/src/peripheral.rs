//! What every platform's peripheral shares: what is advertised, what
//! `info` reads, which writes are allowed, each phone's partial messages,
//! and the ids Herdr UI names phones by. The platform backends only move
//! bytes between Bluetooth and this.

use crate::{
    framing::{Outcome, Reassembly},
    protocol::{Channel, Command, Event, Out},
};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    hash::{BuildHasher, Hasher},
    sync::{
        Arc, Mutex, MutexGuard, PoisonError,
        mpsc::{Receiver, RecvTimeoutError},
    },
    time::{Duration, Instant},
};

/// How often the desktop's idle time is reported while presence is on.
pub(crate) const IDLE_EVERY: Duration = Duration::from_secs(30);

/// Moves bytes between Bluetooth and [`Core`]; one per platform, and a
/// recording one in tests.
pub(crate) trait Backend {
    /// Advertises or stops, as pairing or presence now wants.
    fn refresh(&self, advertise: bool);
    /// Sends `chunks` to the phone the platform knows as `device`.
    fn notify(&self, device: &str, channel: Channel, chunks: Vec<Vec<u8>>);
    /// Seconds since the desktop's last input, or `None` when the platform
    /// cannot tell.
    fn idle_seconds(&self) -> Option<u64>;
}

/// Reported when the desktop cannot say how long it has been idle: long
/// enough that a phone never counts as at an active desk, so presence
/// never holds its alerts.
pub(crate) const IDLE_UNKNOWN: u64 = 365 * 24 * 60 * 60;
// Herdr UI counts a desk as in use under 300 s of idle.
const _: () = assert!(IDLE_UNKNOWN > 300);

#[derive(Debug)]
pub(crate) struct Core {
    /// Pairing's advertisement: the computer's name and fingerprint prefix.
    pairing: Option<(String, String)>,
    presence: bool,
    incoming: HashMap<(String, Channel), Reassembly>,
    /// The id Herdr UI names each phone by, from the platform's own key
    /// (a BlueZ device path, a Windows device id).
    ids: HashMap<String, String>,
    /// The platform key for each id.
    devices: HashMap<String, String>,
    /// The largest chunk each phone takes, as its link reported.
    chunk: HashMap<String, usize>,
    /// Phones that wrote on `presence`, which hear `disconnected` when
    /// they leave.
    present: HashSet<String>,
    seed: u64,
}

#[derive(Serialize)]
struct Info<'a> {
    v: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fp: Option<&'a str>,
}

impl Core {
    pub(crate) fn new(seed: u64) -> Self {
        Self {
            pairing: None,
            presence: false,
            incoming: HashMap::new(),
            ids: HashMap::new(),
            devices: HashMap::new(),
            chunk: HashMap::new(),
            present: HashSet::new(),
            seed,
        }
    }

    /// A seed no other run shares, so phone ids differ between runs.
    pub(crate) fn random_seed() -> u64 {
        let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
        hasher.write_u32(std::process::id());
        hasher.finish()
    }

    pub(crate) fn wants_advertising(&self) -> bool {
        self.pairing.is_some() || self.presence
    }

    /// What `info` reads: the name only while pairing, and nothing at all
    /// when neither pairing nor presence is on.
    pub(crate) fn info(&self) -> Option<Vec<u8>> {
        let info = match &self.pairing {
            Some((name, fp)) => Info {
                v: 1,
                name: Some(name),
                fp: Some(fp),
            },
            None if self.presence => Info {
                v: 1,
                name: None,
                fp: None,
            },
            None => return None,
        };
        serde_json::to_vec(&info).ok()
    }

    pub(crate) fn write_allowed(&self, channel: Channel) -> bool {
        match channel {
            Channel::Pair => self.pairing.is_some(),
            Channel::Presence => self.presence,
        }
    }

    /// The id Herdr UI knows `device` by, shaped like the UUID Core
    /// Bluetooth gives a phone, made the first time it is seen.
    pub(crate) fn id(&mut self, device: &str) -> String {
        if let Some(id) = self.ids.get(device) {
            return id.clone();
        }
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        hasher.write_u64(self.seed);
        hasher.write(device.as_bytes());
        let high = hasher.finish();
        hasher.write_u64(high);
        let low = hasher.finish();
        let id = format!(
            "{:08X}-{:04X}-4{:03X}-8{:03X}-{:012X}",
            high >> 32,
            (high >> 16) & 0xffff,
            high & 0xfff,
            (low >> 48) & 0xfff,
            low & 0xffff_ffff_ffff,
        );
        self.ids.insert(device.to_owned(), id.clone());
        self.devices.insert(id.clone(), device.to_owned());
        id
    }

    /// The platform key of the phone Herdr UI calls `id`.
    pub(crate) fn device(&self, id: &str) -> Option<&str> {
        self.devices.get(id).map(String::as_str)
    }

    /// The link's largest write or notification for `device`.
    pub(crate) fn set_chunk(&mut self, device: &str, size: usize) {
        self.chunk.insert(device.to_owned(), size);
    }

    pub(crate) fn chunk(&self, device: &str) -> usize {
        self.chunk
            .get(device)
            .copied()
            .unwrap_or(crate::framing::MIN_CHUNK)
    }

    /// A chunk a phone wrote; a whole message becomes an event for Herdr
    /// UI. Writes the current mode does not allow are refused before this.
    pub(crate) fn received(
        &mut self,
        device: &str,
        channel: Channel,
        chunk: &[u8],
    ) -> Option<Event> {
        let id = self.id(device);
        if channel == Channel::Presence {
            self.present.insert(device.to_owned());
        }
        let key = (device.to_owned(), channel);
        match self.incoming.entry(key.clone()).or_default().add(chunk) {
            Outcome::Partial => None,
            Outcome::Complete(message) => {
                self.incoming.remove(&key);
                Some(Event::frame(&id, channel, &message))
            }
            Outcome::Invalid => {
                self.incoming.remove(&key);
                None
            }
        }
    }

    /// A phone unsubscribed from `channel` or left; leaving presence is
    /// news for Herdr UI.
    pub(crate) fn left(&mut self, device: &str, channel: Channel) -> Option<Event> {
        self.incoming.remove(&(device.to_owned(), channel));
        if channel != Channel::Presence || !self.present.remove(device) {
            return None;
        }
        Some(Event::Disconnected {
            central: self.id(device),
        })
    }

    /// Applies one command; answers what the radio has to do for it.
    fn apply(&mut self, command: Command) -> Step {
        match command {
            Command::Advertise { name, fingerprint } => self.pairing = Some((name, fingerprint)),
            Command::Presence(on) => self.presence = on,
            Command::Stop => self.pairing = None,
            Command::Frame {
                central,
                channel,
                data,
            } => {
                // A frame for a phone this run never heard from goes nowhere.
                let Some(device) = self.device(&central).map(str::to_owned) else {
                    return Step::Nothing;
                };
                let chunks = crate::framing::chunks(&data, self.chunk(&device));
                return Step::Send(device, channel, chunks);
            }
        }
        Step::Refresh(self.wants_advertising())
    }

    pub(crate) fn presence(&self) -> bool {
        self.presence
    }

    #[cfg(test)]
    pub(crate) fn apply_for_tests(&mut self, command: Command) {
        let _ = self.apply(command);
    }
}

/// What the radio does after a command.
#[derive(Debug, Eq, PartialEq)]
enum Step {
    Refresh(bool),
    Send(String, Channel, Vec<Vec<u8>>),
    Nothing,
}

/// `core` shared between the command loop and the platform's callbacks.
pub(crate) type Shared = Arc<Mutex<Core>>;

pub(crate) fn lock(core: &Shared) -> MutexGuard<'_, Core> {
    core.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Applies Herdr UI's commands until its input ends, and reports the
/// desktop's idle time every [`IDLE_EVERY`] while presence is on. The lock
/// is never held while the backend works, as its callbacks take it too.
pub(crate) fn serve(
    core: &Shared,
    backend: &impl Backend,
    commands: &Receiver<Command>,
    out: &Out,
) {
    let mut next_idle: Option<Instant> = None;
    loop {
        let wait = next_idle.map_or(Duration::from_secs(3600), |at| {
            at.saturating_duration_since(Instant::now())
        });
        match commands.recv_timeout(wait) {
            Ok(command) => {
                let (step, presence_was, presence) = {
                    let mut core = lock(core);
                    let was = core.presence();
                    let step = core.apply(command);
                    (step, was, core.presence())
                };
                match step {
                    Step::Refresh(advertise) => backend.refresh(advertise),
                    Step::Send(device, channel, chunks) => backend.notify(&device, channel, chunks),
                    Step::Nothing => {}
                }
                if presence && !presence_was {
                    report_idle(backend, out);
                    next_idle = Some(Instant::now() + IDLE_EVERY);
                } else if !presence {
                    next_idle = None;
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if next_idle.is_some() {
                    report_idle(backend, out);
                    next_idle = Some(Instant::now() + IDLE_EVERY);
                }
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn report_idle(backend: &impl Backend, out: &Out) {
    out.emit(&Event::Idle {
        seconds: backend.idle_seconds().unwrap_or(IDLE_UNKNOWN),
    });
}
