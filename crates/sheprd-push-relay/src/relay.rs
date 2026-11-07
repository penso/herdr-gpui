//! The relay's decisions: every check a push or revocation passes, in order,
//! and the reply each gives. Pure of HTTP, with APNs behind [`Gateway`] so
//! tests can stand in for it.

use crate::{
    credential::{Credential, DeviceId, Issuer},
    handle::{Handle, Keyring},
    limit::{Limiter, Rates, source_key},
    store::{Memory, Revoked, Store},
};
use serde::{Deserialize, Serialize};
use sheprd_apns::{DeviceToken, Host, MAX_PAYLOAD, Outcome, Priority, Push, PushType, Sender};
use std::{
    future::Future,
    net::IpAddr,
    sync::{Mutex, PoisonError},
};

/// APNs's longest `apns-collapse-id`.
const MAX_COLLAPSE_ID: usize = 64;

/// Where pushes go: APNs, or a stand-in in tests.
pub trait Gateway: Send + Sync + 'static {
    fn post(
        &self,
        token: &DeviceToken,
        push: &Push,
        now_s: u64,
    ) -> impl Future<Output = sheprd_apns::Result<Outcome>> + Send;
}

impl Gateway for Sender {
    async fn post(
        &self,
        token: &DeviceToken,
        push: &Push,
        now_s: u64,
    ) -> sheprd_apns::Result<Outcome> {
        Sender::post(self, token, push, now_s).await
    }
}

/// `POST /v1/push`. It has no topic or host: the relay sets both.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PushRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collapse_id: Option<String>,
    pub handle: String,
    /// The APNs payload as JSON text.
    pub payload: String,
    pub priority: Priority,
    pub push_type: PushType,
}

/// `POST /v1/revoke`.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RevokeRequest {
    pub handle: String,
}

/// What a caller is told. `Refused` covers every bad handle alike, so a
/// caller learns nothing about why.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reply {
    Delivered,
    /// The credential ran out: the phone must renew it. Keep the handle's
    /// place, since a new one will come.
    Expired,
    /// The handle will never work again: the daemon forgets it.
    Gone,
    /// The request is malformed.
    Invalid,
    Limited,
    Refused,
    Revoked,
    /// The relay or APNs cannot take it now. Not worth queueing.
    Unavailable,
}

struct Limits {
    device: Limiter<DeviceId>,
    handle: Limiter<String>,
    revoke: Limiter<DeviceId>,
    source: Limiter<IpAddr>,
}

/// One relay instance serves one APNs environment.
pub struct Relay<G, S = Memory> {
    environment: Host,
    gateway: G,
    issuer: Issuer,
    keys: Keyring,
    limits: Mutex<Limits>,
    store: S,
}

impl<G: Gateway, S: Store> Relay<G, S> {
    #[must_use]
    pub fn new(
        gateway: G,
        environment: Host,
        issuer: Issuer,
        keys: Keyring,
        store: S,
        rates: Rates,
    ) -> Self {
        Self {
            environment,
            gateway,
            issuer,
            keys,
            limits: Mutex::new(Limits {
                device: Limiter::new(rates.device),
                handle: Limiter::new(rates.handle),
                revoke: Limiter::new(rates.revoke),
                source: Limiter::new(rates.source),
            }),
            store,
        }
    }

    fn limits(&self) -> std::sync::MutexGuard<'_, Limits> {
        self.limits.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Before any cryptography, so a flood of junk costs no key agreement.
    pub fn source_allowed(&self, source: IpAddr, now_ms: u64) -> bool {
        self.limits().source.allow(source_key(source), now_ms)
    }

    /// The opened handle and its credential, or the reply that ends the
    /// request. A revoked handle is checked by [`Self::push`] alone: revoking
    /// one twice should not fail.
    fn opened(&self, text: &str) -> Result<(Handle, Credential), Reply> {
        let handle = self.keys.open(text).ok_or(Reply::Refused)?;
        let credential = self
            .issuer
            .check(&handle.credential)
            .filter(|credential| credential.environment == self.environment)
            .ok_or(Reply::Refused)?;
        Ok((handle, credential))
    }

    /// Sends one push, at `now_ms`, for a caller at `source`.
    pub async fn push(&self, source: IpAddr, request: PushRequest, now_ms: u64) -> Reply {
        if !self.source_allowed(source, now_ms) {
            return Reply::Limited;
        }
        let (handle, credential) = match self.opened(&request.handle) {
            Ok(opened) => opened,
            Err(reply) => return reply,
        };
        let now_s = now_ms / 1000;
        if credential.expires_s <= now_s {
            return Reply::Expired;
        }
        match self.store.is_revoked(credential.device, &handle.id).await {
            Ok(false) => {}
            Ok(true) => return Reply::Gone,
            // A revocation that cannot be checked may be in force.
            Err(error) => {
                tracing::error!(%error, "could not check revocations");
                return Reply::Unavailable;
            }
        }
        if !handle.kind.allows(request.push_type) {
            return Reply::Refused;
        }
        if !valid_payload(&request.payload) || !valid_collapse_id(request.collapse_id.as_deref()) {
            return Reply::Invalid;
        }
        {
            let mut limits = self.limits();
            if !limits.device.allow(credential.device, now_ms)
                || !limits.handle.allow(handle.id, now_ms)
            {
                return Reply::Limited;
            }
        }
        let push = Push {
            body: request.payload.into_bytes(),
            collapse_id: request.collapse_id,
            priority: request.priority,
            push_type: request.push_type,
        };
        match self.gateway.post(&handle.token, &push, now_s).await {
            Ok(outcome) => reply(outcome),
            Err(error) => {
                tracing::warn!(%error, "could not reach APNs");
                Reply::Unavailable
            }
        }
    }

    /// Revokes one handle, for its daemon when a phone unpairs. Holding the
    /// handle is the authority: only that daemon does.
    pub async fn revoke(&self, source: IpAddr, request: RevokeRequest, now_ms: u64) -> Reply {
        if !self.source_allowed(source, now_ms) {
            return Reply::Limited;
        }
        let (handle, credential) = match self.opened(&request.handle) {
            Ok(opened) => opened,
            Err(reply) => return reply,
        };
        // Nothing to keep: the relay refuses this credential already.
        if credential.expires_s <= now_ms / 1000 {
            return Reply::Revoked;
        }
        let allowed = {
            let mut limits = self.limits();
            limits.device.allow(credential.device, now_ms)
                && limits.revoke.allow(credential.device, now_ms)
        };
        if !allowed {
            return Reply::Limited;
        }
        let what = Revoked::Handle(handle.id);
        match self.store.revoke(&what, credential.expires_s).await {
            Ok(()) => Reply::Revoked,
            Err(error) => {
                tracing::error!(%error, "could not store a revocation");
                Reply::Unavailable
            }
        }
    }
}

/// What the caller hears of APNs's answer. The reason is one of Apple's fixed
/// names, so it is safe to log.
fn reply(outcome: Outcome) -> Reply {
    match outcome {
        Outcome::Delivered => Reply::Delivered,
        Outcome::Gone => Reply::Gone,
        Outcome::RateLimited => Reply::Limited,
        Outcome::Rejected { reason, status } => {
            tracing::warn!(status, reason = reason.as_deref(), "APNs refused a push");
            // 403 is the relay's own key, and 5xx is Apple: neither is the
            // caller's fault, and a later push may work.
            if status == 403 || status >= 500 {
                Reply::Unavailable
            } else {
                Reply::Refused
            }
        }
    }
}

/// A JSON object within APNs's bound.
fn valid_payload(payload: &str) -> bool {
    payload.len() <= MAX_PAYLOAD
        && serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(payload).is_ok()
}

/// What can go in a header and APNs takes.
fn valid_collapse_id(id: Option<&str>) -> bool {
    id.is_none_or(|id| {
        (1..=MAX_COLLAPSE_ID).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_graphic())
    })
}
