//! Where the pairing dialog stands, as a state machine with no I/O: it says
//! which control request to send next, and takes each answer. The dialog
//! runs the requests off the UI thread and feeds the answers back, so every
//! transition here is tested without a daemon or a window.
//!
//! A pairing shows a QR whose token lasts a minute. When it runs out the
//! dialog asks for another; once a phone claims it, the dialog shows the
//! phone's name and matching code and waits for Allow or Deny.

use super::qr::Qr;
use crate::control::{self, ErrorCode, Offer, PairingId, PairingState, Request, Response};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// How often the dialog asks where the pairing stands.
pub const POLL: Duration = Duration::from_secs(1);

#[derive(Clone, Debug, PartialEq)]
pub enum Phase {
    /// Remote control is off in Settings, so there is no daemon to ask.
    Off,
    /// Asking the daemon for a QR, the first or a fresh one.
    Requesting,
    /// Showing the QR until `deadline`, when a fresh one is asked for.
    Showing {
        offer: Offer,
        qr: Arc<Qr>,
        deadline: Instant,
        lifetime: Duration,
    },
    /// A phone claimed the QR: the user compares codes and answers.
    Claimed {
        offer: Offer,
        device_name: String,
        code: String,
        /// `PairingState::Claimed::app`: another computer's app claimed it.
        app: Option<String>,
        /// The answer given, once sent: `Some(true)` waits for the daemon
        /// to report the device approved.
        answer: Option<Answer>,
    },
    Approved {
        device_name: String,
    },
    Denied,
    /// The claimed pairing ran out before an answer, or was voided.
    Expired {
        device_name: String,
    },
    Failed {
        message: String,
    },
}

/// The user's answer to a claim, and whether it has gone to the daemon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Answer {
    pub allow: bool,
    pub sent: bool,
}

/// One request in flight, so its answer is matched to the state that sent it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ticket {
    epoch: u64,
    pub request: Request,
}

#[derive(Clone, Debug)]
pub struct Pairing {
    phase: Phase,
    /// Raised by every change the user makes, so an answer to a request sent
    /// before it is dropped.
    epoch: u64,
    in_flight: bool,
    last_poll: Option<Instant>,
}

impl Pairing {
    /// `enabled` is whether remote control is on in Settings.
    pub fn new(enabled: bool) -> Self {
        Self {
            phase: if enabled {
                Phase::Requesting
            } else {
                Phase::Off
            },
            epoch: 0,
            in_flight: false,
            last_poll: None,
        }
    }

    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    /// The pairing the daemon holds open for this dialog, if any.
    pub fn id(&self) -> Option<&PairingId> {
        match &self.phase {
            Phase::Showing { offer, .. } | Phase::Claimed { offer, .. } => Some(&offer.id),
            _ => None,
        }
    }

    /// Whether closing the dialog should cancel its pairing: it is still
    /// open at the daemon, and no device came of it.
    pub fn cancels_on_close(&self) -> bool {
        self.id().is_some()
    }

    /// The request to send now, if any. At most one is in flight.
    pub fn next(&mut self, now: Instant) -> Option<Ticket> {
        if self.in_flight {
            return None;
        }
        let poll_due = self
            .last_poll
            .is_none_or(|last| now.saturating_duration_since(last) >= POLL);
        let request = match &self.phase {
            Phase::Requesting => Request::Pair,
            // The daemon voids the old token when it mints the next one.
            Phase::Showing { deadline, .. } if now >= *deadline => Request::Pair,
            Phase::Claimed {
                offer,
                code,
                answer: Some(Answer { allow, sent: false }),
                ..
            } => Request::Approve {
                id: offer.id.clone(),
                allow: *allow,
                // The daemon refuses an approval for any claim but this one.
                code: code.clone(),
            },
            Phase::Showing { offer, .. } | Phase::Claimed { offer, .. } if poll_due => {
                Request::Pairing {
                    id: offer.id.clone(),
                }
            }
            _ => return None,
        };
        self.in_flight = true;
        Some(Ticket {
            epoch: self.epoch,
            request,
        })
    }

    /// Takes the answer to `ticket`. One sent before the user changed
    /// something is dropped.
    pub fn receive(
        &mut self,
        ticket: &Ticket,
        result: Result<Response, control::Error>,
        now: Instant,
    ) {
        if ticket.epoch != self.epoch {
            return;
        }
        self.in_flight = false;
        if matches!(ticket.request, Request::Pairing { .. }) {
            self.last_poll = Some(now);
        }
        let response = match result {
            Ok(response) => response,
            // The pairing is gone: a QR still showing is replaced, a claim
            // that was waiting is over.
            Err(control::Error::Daemon {
                code: ErrorCode::UnknownPairing,
                ..
            }) => {
                self.lost();
                return;
            }
            Err(error) => {
                self.phase = Phase::Failed {
                    message: error.to_string(),
                };
                return;
            }
        };
        match (&ticket.request, response) {
            (Request::Pair, Response::Offer(offer)) => self.show(offer, now),
            (Request::Pairing { id }, Response::Pairing(state)) if self.id() == Some(id) => {
                self.update(state, now);
            }
            // The daemon answers with the pairing's state after the answer.
            (Request::Approve { id, allow, .. }, Response::Pairing(state))
                if self.id() == Some(id) =>
            {
                if let Phase::Claimed { answer, .. } = &mut self.phase {
                    *answer = Some(Answer {
                        allow: *allow,
                        sent: true,
                    });
                }
                self.update(state, now);
            }
            // A poll for a pairing since replaced.
            (Request::Pairing { .. }, Response::Pairing(_)) => {}
            _ => {
                self.phase = Phase::Failed {
                    message: crate::app::Error::Unexpected.to_string(),
                };
            }
        }
    }

    fn show(&mut self, offer: Offer, now: Instant) {
        let lifetime = Duration::from_millis(offer.expires_in_ms);
        self.last_poll = Some(now);
        self.phase = match Qr::encode(&offer.url) {
            Ok(qr) => Phase::Showing {
                deadline: now + lifetime,
                lifetime,
                qr: Arc::new(qr),
                offer,
            },
            Err(error) => Phase::Failed {
                message: error.to_string(),
            },
        };
    }

    fn update(&mut self, state: PairingState, now: Instant) {
        let phase = std::mem::replace(&mut self.phase, Phase::Requesting);
        self.phase = match (phase, state) {
            (
                Phase::Showing {
                    offer,
                    qr,
                    lifetime,
                    ..
                },
                PairingState::Waiting { expires_in_ms },
            ) => Phase::Showing {
                // The daemon's clock decides when the token stops working.
                deadline: now + Duration::from_millis(expires_in_ms),
                offer,
                qr,
                lifetime,
            },
            (
                Phase::Showing { offer, .. },
                PairingState::Claimed {
                    device_name,
                    code,
                    app,
                },
            ) => Phase::Claimed {
                offer,
                device_name,
                code,
                app,
                answer: None,
            },
            (Phase::Claimed { device_name, .. }, PairingState::Approved { .. }) => {
                Phase::Approved { device_name }
            }
            (Phase::Showing { .. }, PairingState::Approved { .. }) => Phase::Approved {
                device_name: "The device".into(),
            },
            (_, PairingState::Denied) => Phase::Denied,
            (phase, PairingState::Expired) => {
                self.phase = phase;
                self.lost();
                return;
            }
            (phase, _) => phase,
        };
    }

    /// The daemon no longer holds the pairing.
    fn lost(&mut self) {
        self.phase = match std::mem::replace(&mut self.phase, Phase::Requesting) {
            Phase::Claimed { device_name, .. } => Phase::Expired { device_name },
            Phase::Showing { .. } | Phase::Requesting => Phase::Requesting,
            phase => phase,
        };
    }

    /// The user's Allow or Deny for the claimed device.
    pub fn answer(&mut self, allow: bool) {
        if let Phase::Claimed { answer, .. } = &mut self.phase
            && answer.is_none()
        {
            *answer = Some(Answer { allow, sent: false });
            self.epoch += 1;
            self.in_flight = false;
        }
    }

    /// A dialog that cannot reach a daemon at all, saying why.
    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            phase: Phase::Failed {
                message: message.into(),
            },
            ..Self::new(true)
        }
    }

    /// Starts over with a fresh QR, after an expiry or a failure.
    pub fn restart(&mut self) {
        if matches!(self.phase, Phase::Off) {
            return;
        }
        self.phase = Phase::Requesting;
        self.epoch += 1;
        self.in_flight = false;
        self.last_poll = None;
    }

    /// How long the QR has left, and that as a share of its lifetime.
    pub fn countdown(&self, now: Instant) -> Option<(Duration, f32)> {
        let Phase::Showing {
            deadline, lifetime, ..
        } = &self.phase
        else {
            return None;
        };
        let left = deadline.saturating_duration_since(now);
        let share = if lifetime.is_zero() {
            0.
        } else {
            (left.as_secs_f32() / lifetime.as_secs_f32()).clamp(0., 1.)
        };
        Some((left, share))
    }
}

/// The matching code as people read it: `428913` as `428 913`.
pub fn spaced(code: &str) -> String {
    let chars: Vec<char> = code.chars().filter(|c| !c.is_whitespace()).collect();
    if chars.len() != 6 {
        return code.to_owned();
    }
    let (first, second) = chars.split_at(3);
    format!(
        "{} {}",
        first.iter().collect::<String>(),
        second.iter().collect::<String>()
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
