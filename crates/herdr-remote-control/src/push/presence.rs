//! Holding a phone's alerts while its owner is at the computer
//! (`docs/ios-app-plan.md`, "Presence, exactly"). Herdr UI marks a device
//! near while its proven phone is in Bluetooth range and the desktop is in
//! use; the notifier holds that device's alerts, Web Push and APNs alike,
//! and lets them go when the mark is lifted or lapses. Live Activities are
//! not held: they update quietly.

use super::Notification;
use crate::control::DeviceId;
use std::collections::{HashMap, VecDeque};

/// How long a near mark lasts without being renewed; Herdr UI renews it
/// every 30 s, so a Herdr UI that quit or crashed lets alerts through.
pub(crate) const NEAR_TTL_MS: u64 = 120_000;
/// Alerts held per device; the oldest go first when more arrive.
pub(crate) const MAX_HELD: usize = 32;

/// Which devices are near, until when.
#[derive(Debug, Default)]
pub(crate) struct Near {
    until: HashMap<DeviceId, u64>,
}

impl Near {
    pub(crate) fn set(&mut self, device: &DeviceId, near: bool, now_ms: u64) {
        if near {
            self.until
                .insert(device.clone(), now_ms.saturating_add(NEAR_TTL_MS));
        } else {
            self.until.remove(device);
        }
    }

    pub(crate) fn is_near(&self, device: &DeviceId, now_ms: u64) -> bool {
        self.until.get(device).is_some_and(|until| *until > now_ms)
    }

    /// Drops marks that lapsed.
    pub(crate) fn forget(&mut self, now_ms: u64) {
        self.until.retain(|_, until| *until > now_ms);
    }
}

/// One device's held alerts, oldest first, at most one per chat.
#[derive(Debug, Default)]
pub(crate) struct Hold {
    held: VecDeque<Notification>,
}

impl Hold {
    /// Holds `notification`, replacing an older one for the same chat.
    /// A clear is not held: it drops what was held for its chat and is
    /// returned to send now, so a phone that already shows that chat's
    /// question loses it.
    pub(crate) fn hold(&mut self, notification: Notification) -> Option<Notification> {
        self.held.retain(|held| held.tag != notification.tag);
        if notification.clear {
            return Some(notification);
        }
        self.held.push_back(notification);
        while self.held.len() > MAX_HELD {
            self.held.pop_front();
        }
        None
    }

    /// Everything held, oldest first, to send now.
    pub(crate) fn release(&mut self) -> Vec<Notification> {
        self.held.drain(..).collect()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.held.is_empty()
    }
}
