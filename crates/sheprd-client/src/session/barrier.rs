//! A surface activation written behind an unacknowledged synchronous focus
//! request. Activation advances the revision that fences deferred navigation,
//! so it must never follow any other request early.

use super::{Pending, Session};
use crate::{handle::Command, method::Method};
use serde_json::Value;

pub(crate) struct Barrier {
    pub(crate) pending: Pending,
    pub(crate) request: Vec<u8>,
    /// Delivered only after the focus response, preserving send order. Its
    /// raw bytes remain in `pending` and count toward the response bound.
    pub(crate) held: Option<Value>,
    /// A daemon without the busy-check exemption refused it unprocessed.
    pub(crate) refused: bool,
}

impl Barrier {
    pub(super) fn awaits(&self, request_id: &str) -> bool {
        self.pending.id == request_id && self.held.is_none() && !self.refused
    }
}

/// The exact stable releases whose sources were checked; any other string,
/// including previews, later versions, and other spellings, stays serialized.
pub(super) fn verified_ordering(server_version: &str) -> bool {
    matches!(
        server_version,
        "0.9.0" | "0.9.1" | "0.9.2" | "0.9.3" | "0.9.4" | "0.9.5"
    )
}

pub(super) fn endpoint_busy(response: &Value) -> bool {
    response
        .pointer("/error/code")
        .is_some_and(|code| code == "endpoint_busy")
}

impl Session {
    pub(crate) fn pipelines(&self, command: &Command) -> bool {
        self.pipeline_barriers
            && self.barrier.is_none()
            && command
                .request
                .as_ref()
                .is_some_and(|request| request.method == Method::ClientShellSurfaceSet)
            && self.pending.as_ref().is_some_and(|pending| {
                matches!(
                    pending.method,
                    Method::PaneFocus | Method::TabFocus | Method::WorkspaceFocus
                )
            })
    }

    /// A refused activation is resent unchanged once focus completes. Focus is
    /// never resent; a reboot or cancellation ends the connection and this state.
    pub(crate) fn take_refused_barrier(&mut self) -> Option<Barrier> {
        if !self.resend_ready() {
            return None;
        }
        self.barrier.take()
    }

    /// Focus has completed and its refused activation waits to be resent.
    pub(crate) fn resend_ready(&self) -> bool {
        self.pending.is_none() && self.barrier.as_ref().is_some_and(|b| b.refused)
    }
}
