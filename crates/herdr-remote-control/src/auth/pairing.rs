//! The pairing the desktop dialog holds open: a single-use token in its QR,
//! a phone's claim of it, and the person's answer.
//!
//! Only the token's digest is kept. At most one pairing is open; minting
//! again rotates its token, voiding the previous one.

use super::{
    Error, PublicKey, Result, SECRET_LEN, constant_time_eq, decode_fixed, device_name, encode,
    random_bytes, random_id, sha256,
};
use crate::control::{DeviceId, Offer, PairingId, PairingState};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// How long a minted token can be claimed.
pub(crate) const TOKEN_TTL_MS: u64 = 60_000;
/// How long a claimed pairing waits for the desktop's answer.
pub(crate) const APPROVAL_TTL_MS: u64 = 120_000;
/// Finished claims whose outcome a phone can still poll.
const REMEMBERED_OUTCOMES: usize = 8;

/// A phone's claim, named by the daemon so the phone can poll its outcome.
#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct ClaimId(pub(crate) String);

/// What the phone learns when its claim is accepted.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct Claim {
    pub(crate) id: ClaimId,
    /// The matching code the phone shows beside the desktop's.
    pub(crate) code: String,
    /// When the desktop's answer stops being accepted.
    pub(crate) expires_ms: u64,
}

/// A device the desktop approved, for the device store to keep.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NewDevice {
    pub(crate) id: DeviceId,
    pub(crate) name: String,
    pub(crate) public_key: PublicKey,
}

#[derive(Debug, Default)]
pub(crate) struct Pairings {
    open: Option<Open>,
    /// Outcomes of finished claims, oldest first.
    outcomes: VecDeque<(ClaimId, PairingState)>,
}

#[derive(Debug)]
struct Open {
    id: PairingId,
    token_digest: [u8; 32],
    token_expires_ms: u64,
    phase: Phase,
}

#[derive(Debug)]
enum Phase {
    Waiting,
    Claimed {
        claim: ClaimId,
        device: NewDevice,
        code: String,
        deadline_ms: u64,
    },
    /// Approved, denied, or expired; `claim` is `None` when the token
    /// expired unclaimed.
    Finished {
        claim: Option<ClaimId>,
        state: PairingState,
    },
}

impl Open {
    /// Whether minting again keeps this pairing's id: nobody claimed it.
    fn unclaimed(&self) -> bool {
        matches!(
            self.phase,
            Phase::Waiting | Phase::Finished { claim: None, .. }
        )
    }

    fn view(&self, now_ms: u64) -> PairingState {
        match &self.phase {
            Phase::Waiting => PairingState::Waiting {
                expires_in_ms: self.token_expires_ms.saturating_sub(now_ms),
            },
            Phase::Claimed { device, code, .. } => PairingState::Claimed {
                device_name: device.name.clone(),
                code: code.clone(),
            },
            Phase::Finished { state, .. } => state.clone(),
        }
    }
}

impl Pairings {
    /// Opens a pairing, or rotates the open one's token while nobody has
    /// claimed it. A claimed or finished pairing is replaced by a new one.
    pub(crate) fn mint(
        &mut self,
        now_ms: u64,
        origins: &[String],
        fingerprint: &str,
    ) -> Result<Offer> {
        let origin = origins.first().ok_or(Error::NoAddress)?;
        let token = random_bytes::<SECRET_LEN>()?;
        let id = match &self.open {
            Some(open) if open.unclaimed() => open.id.clone(),
            _ => PairingId(random_id()?),
        };
        self.close();
        let token_expires_ms = now_ms.saturating_add(TOKEN_TTL_MS);
        self.open = Some(Open {
            id: id.clone(),
            token_digest: sha256(&[&token]),
            token_expires_ms,
            phase: Phase::Waiting,
        });
        Ok(Offer {
            id,
            url: format!(
                "{origin}/pair#t={}&fp={}",
                encode(&token),
                fingerprint.replace(':', "")
            ),
            fingerprint: fingerprint.to_owned(),
            expires_in_ms: TOKEN_TTL_MS,
            addresses: origins.to_vec(),
        })
    }

    /// A phone claims the token with its name and public key. The token is
    /// used up only when the name and key are valid; claiming it twice
    /// voids the pairing.
    pub(crate) fn claim(
        &mut self,
        token: &str,
        device_name_input: &str,
        public_key: &str,
        now_ms: u64,
    ) -> Result<Claim> {
        let token: [u8; SECRET_LEN] = decode_fixed(token).map_err(|_| Error::InvalidToken)?;
        let digest = sha256(&[&token]);
        let Some(open) = self.open.as_mut() else {
            return Err(Error::InvalidToken);
        };
        if !constant_time_eq(&open.token_digest, &digest) {
            return Err(Error::InvalidToken);
        }
        refresh(open, &mut self.outcomes, now_ms);
        match &open.phase {
            Phase::Waiting => {}
            Phase::Finished { claim: None, .. } => return Err(Error::ExpiredToken),
            Phase::Finished { claim: Some(_), .. } => return Err(Error::AlreadyClaimed),
            Phase::Claimed { .. } => {
                settle(open, &mut self.outcomes, PairingState::Denied);
                return Err(Error::AlreadyClaimed);
            }
        }
        let name = device_name(device_name_input)?;
        let public_key = PublicKey::parse(public_key)?;
        let nonce = random_bytes::<SECRET_LEN>()?;
        let claim = ClaimId(random_id()?);
        let device = NewDevice {
            id: DeviceId(random_id()?),
            name,
            public_key,
        };
        let code = matching_code(&token, device.public_key.as_bytes(), &nonce);
        let deadline_ms = now_ms.saturating_add(APPROVAL_TTL_MS);
        open.phase = Phase::Claimed {
            claim: claim.clone(),
            device,
            code: code.clone(),
            deadline_ms,
        };
        Ok(Claim {
            id: claim,
            code,
            expires_ms: deadline_ms,
        })
    }

    /// Where the pairing stands, for the desktop dialog.
    pub(crate) fn state(&mut self, id: &PairingId, now_ms: u64) -> Result<PairingState> {
        let open = find(&mut self.open, id)?;
        refresh(open, &mut self.outcomes, now_ms);
        Ok(open.view(now_ms))
    }

    /// Where a claim stands, for the phone that made it.
    pub(crate) fn claim_status(&mut self, id: &ClaimId, now_ms: u64) -> Result<PairingState> {
        if let Some(open) = self.open.as_mut() {
            refresh(open, &mut self.outcomes, now_ms);
            if matches!(&open.phase, Phase::Claimed { claim, .. } if claim == id) {
                return Ok(open.view(now_ms));
            }
        }
        self.outcomes
            .iter()
            .rev()
            .find(|(claim, _)| claim == id)
            .map(|(_, state)| state.clone())
            .ok_or(Error::UnknownClaim)
    }

    /// The desktop's answer to a claimed pairing. On approval `keep` stores
    /// the device before anyone can learn it was approved; when it fails the
    /// pairing is denied instead. Returns the approved device.
    pub(crate) fn answer(
        &mut self,
        id: &PairingId,
        allow: bool,
        now_ms: u64,
        keep: impl FnOnce(&NewDevice) -> Result<()>,
    ) -> Result<Option<NewDevice>> {
        let open = find(&mut self.open, id)?;
        refresh(open, &mut self.outcomes, now_ms);
        let Phase::Claimed { device, .. } = &open.phase else {
            return Err(Error::NotClaimed);
        };
        if !allow {
            settle(open, &mut self.outcomes, PairingState::Denied);
            return Ok(None);
        }
        if let Err(error) = keep(device) {
            settle(open, &mut self.outcomes, PairingState::Denied);
            return Err(error);
        }
        let approved = PairingState::Approved {
            device: device.id.clone(),
        };
        match settle(open, &mut self.outcomes, approved) {
            Phase::Claimed { device, .. } => Ok(Some(device)),
            _ => Err(Error::NotClaimed),
        }
    }

    /// Closes the dialog's pairing; its token stops working and a pending
    /// claim is denied.
    pub(crate) fn cancel(&mut self, id: &PairingId) -> Result<()> {
        find(&mut self.open, id)?;
        self.close();
        Ok(())
    }

    fn close(&mut self) {
        if let Some(mut open) = self.open.take() {
            settle(&mut open, &mut self.outcomes, PairingState::Denied);
        }
    }
}

fn find<'a>(open: &'a mut Option<Open>, id: &PairingId) -> Result<&'a mut Open> {
    open.as_mut()
        .filter(|open| &open.id == id)
        .ok_or(Error::UnknownPairing)
}

/// Expires a token nobody claimed in time, or a claim nobody answered.
fn refresh(open: &mut Open, outcomes: &mut VecDeque<(ClaimId, PairingState)>, now_ms: u64) {
    match open.phase {
        Phase::Waiting if now_ms >= open.token_expires_ms => {
            open.phase = Phase::Finished {
                claim: None,
                state: PairingState::Expired,
            };
        }
        Phase::Claimed { deadline_ms, .. } if now_ms >= deadline_ms => {
            settle(open, outcomes, PairingState::Expired);
        }
        _ => {}
    }
}

/// Finishes a claimed pairing with `state` and records the outcome for the
/// phone; anything else is left alone. Returns the phase it replaced.
fn settle(
    open: &mut Open,
    outcomes: &mut VecDeque<(ClaimId, PairingState)>,
    state: PairingState,
) -> Phase {
    let Phase::Claimed { claim, .. } = &open.phase else {
        return Phase::Waiting;
    };
    let claim = claim.clone();
    if outcomes.len() == REMEMBERED_OUTCOMES {
        outcomes.pop_front();
    }
    outcomes.push_back((claim.clone(), state.clone()));
    std::mem::replace(
        &mut open.phase,
        Phase::Finished {
            claim: Some(claim),
            state,
        },
    )
}

/// The six digits both screens show: the first four bytes of
/// SHA-256(token || public key || server nonce), modulo one million.
pub(crate) fn matching_code(token: &[u8], public_key: &[u8], nonce: &[u8]) -> String {
    let digest = sha256(&[token, public_key, nonce]);
    let value = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
    format!("{:06}", value % 1_000_000)
}
