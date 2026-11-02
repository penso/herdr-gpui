//! Presence, Herdr UI's part (`docs/ios-app-plan.md`, "Presence,
//! exactly"): carries each phone's `presence` frames to the daemon,
//! remembers which phone proved which device, and tells the daemon a
//! device is near while its proven phone says it is and the desktop is in
//! use. The proof and the hold run in the daemon; this only decides near.
//! Pure state: the station feeds it the helper's events and sends what it
//! asks for.

use herdr_remote_control::control::{self, DeviceId, Request, Response};
use serde::Deserialize;
use serde_json::json;
use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

/// Input older than this means nobody is at the desk.
pub(crate) const IDLE_LIMIT_SECONDS: u64 = 300;
/// How often a near mark is renewed; the daemon lets one lapse after two
/// minutes.
pub(crate) const RENEW: Duration = Duration::from_secs(30);
/// Hellos one phone may send per [`HELLO_WINDOW`]; more are dropped, so a
/// device nearby cannot make the daemon sign challenges without end.
pub(crate) const HELLO_LIMIT: usize = 8;
pub(crate) const HELLO_WINDOW: Duration = Duration::from_secs(60);
/// Challenges one phone may hold unanswered, as the daemon keeps per device.
const NONCES_KEPT: usize = 4;
/// What the phone reads for every failure, so it learns nothing from which.
pub(crate) const REFUSED: &str = "presence was not accepted"; // i18n: data

/// A phone's presence message.
#[derive(Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
enum PhoneFrame {
    /// The phone's daily hint, not its device id.
    Hello {
        hint: String,
    },
    Prove {
        nonce: String,
        signature: String,
    },
    Near {
        near: bool,
    },
}

/// What to do with a phone's frame.
#[derive(Debug, PartialEq)]
pub(crate) enum Received {
    /// Ask the daemon, then hand its answer to [`PresenceRelay::answered`].
    Ask(Request),
    /// Write this back to the phone at once.
    Reply(Vec<u8>),
    Nothing,
}

#[derive(Debug, Default)]
struct Central {
    /// Nonces the daemon gave this phone, newest last: it may prove only
    /// one of these.
    nonces: VecDeque<String>,
    /// When its recent hellos came, for [`HELLO_LIMIT`].
    hellos: VecDeque<Instant>,
    /// The device this phone proved it is.
    proven: Option<DeviceId>,
    /// Whether the phone says it is close (its RSSI, its own hysteresis).
    near: bool,
}

#[derive(Debug, Default)]
pub(crate) struct PresenceRelay {
    centrals: HashMap<String, Central>,
    /// Seconds since the desktop's last input, as the helper last said.
    idle: Option<u64>,
    /// What the daemon was last told per device, and when.
    sent: HashMap<DeviceId, (bool, Instant)>,
}

impl PresenceRelay {
    /// A frame phone `central` wrote at `now`.
    pub(crate) fn receive(&mut self, central: &str, data: &[u8], now: Instant) -> Received {
        let Ok(frame) = serde_json::from_slice::<PhoneFrame>(data) else {
            return Received::Reply(refused());
        };
        let state = self.centrals.entry(central.to_owned()).or_default();
        match frame {
            PhoneFrame::Hello { hint } => {
                while state
                    .hellos
                    .front()
                    .is_some_and(|at| now.duration_since(*at) >= HELLO_WINDOW)
                {
                    state.hellos.pop_front();
                }
                if state.hellos.len() >= HELLO_LIMIT {
                    return Received::Nothing;
                }
                state.hellos.push_back(now);
                Received::Ask(Request::PresenceChallenge { hint })
            }
            PhoneFrame::Prove { nonce, signature } => {
                match state.nonces.iter().position(|given| *given == nonce) {
                    Some(index) => {
                        state.nonces.remove(index);
                        Received::Ask(Request::PresenceProve { nonce, signature })
                    }
                    None => Received::Reply(refused()),
                }
            }
            PhoneFrame::Near { near } => {
                state.near = near;
                Received::Nothing
            }
        }
    }

    /// The daemon's answer to what [`Self::receive`] asked, as the phone
    /// reads it.
    pub(crate) fn answered(
        &mut self,
        central: &str,
        result: Result<Response, control::Error>,
    ) -> Vec<u8> {
        let state = self.centrals.entry(central.to_owned()).or_default();
        match result {
            Ok(Response::PresenceChallenge {
                certificate,
                nonce,
                signature,
            }) => {
                if state.nonces.len() >= NONCES_KEPT {
                    state.nonces.pop_front();
                }
                state.nonces.push_back(nonce.clone());
                json!({
                    "type": "challenge",
                    "nonce": nonce,
                    "certificate": certificate,
                    "signature": signature,
                })
                .to_string()
                .into_bytes()
            }
            Ok(Response::PresenceProven { device, addresses }) => {
                state.proven = Some(device);
                json!({ "type": "proven", "addresses": addresses })
                    .to_string()
                    .into_bytes()
            }
            // The daemon's reason stays here: the phone reads one text.
            Ok(_) | Err(_) => refused(),
        }
    }

    /// Seconds since the desktop's last input.
    pub(crate) fn idle(&mut self, seconds: u64) {
        self.idle = Some(seconds);
    }

    /// Phone `central` went out of range or closed the connection.
    pub(crate) fn disconnected(&mut self, central: &str) {
        self.centrals.remove(central);
    }

    /// The near marks to send now: each change, and a renewal every
    /// [`RENEW`] while near. A device is near while any of its proven
    /// phones says so and the desktop is in use.
    pub(crate) fn due(&mut self, now: Instant) -> Vec<Request> {
        let at_desk = self.idle.is_some_and(|idle| idle < IDLE_LIMIT_SECONDS);
        let mut wanted: HashMap<DeviceId, bool> = self
            .sent
            .keys()
            .map(|device| (device.clone(), false))
            .collect();
        for central in self.centrals.values() {
            if let Some(device) = &central.proven {
                let near = wanted.entry(device.clone()).or_insert(false);
                *near |= central.near && at_desk;
            }
        }
        let mut requests = Vec::new();
        for (device, near) in wanted {
            let send = match self.sent.get(&device) {
                None => near,
                Some((sent, at)) => *sent != near || (near && now.duration_since(*at) >= RENEW),
            };
            if send {
                requests.push(Request::PresenceSet {
                    device: device.clone(),
                    near,
                });
            }
            if near {
                if send {
                    self.sent.insert(device, (near, now));
                }
            } else {
                self.sent.remove(&device);
            }
        }
        requests
    }
}

fn refused() -> Vec<u8> {
    json!({ "type": "error", "message": REFUSED })
        .to_string()
        .into_bytes()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
