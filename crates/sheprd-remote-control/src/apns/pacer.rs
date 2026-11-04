//! Paces Live Activity updates for one device: a start, an end, and a
//! question go at once; step updates for a chat go at most every
//! [`STEP_INTERVAL_MS`], keeping only the newest held back. iOS gives each
//! app a budget of updates and throttles past it, and a step that changes
//! every second is unreadable anyway.
//!
//! Times are explicit milliseconds. Memory is bounded by the chats with a
//! running turn: a chat is forgotten when its activity ends.

use super::{Event, Live};
use std::collections::HashMap;

/// The shortest gap between two step updates for one chat.
pub(crate) const STEP_INTERVAL_MS: u64 = 10_000;

#[derive(Debug, Default)]
pub(crate) struct Pacer {
    chats: HashMap<String, Recent>,
}

#[derive(Debug)]
struct Recent {
    last_ms: u64,
    pending: Option<Live>,
}

impl Pacer {
    /// Returns `live` when it may go at `now_ms`, or holds it back,
    /// replacing an older held update for the chat.
    pub(crate) fn offer(&mut self, live: Live, now_ms: u64) -> Option<Live> {
        match live.event {
            Event::End { .. } => {
                self.chats.remove(&live.pane);
                return Some(live);
            }
            Event::Start { .. } => {}
            Event::Update { .. } if live.urgent() => {}
            Event::Update { .. } => {
                if let Some(recent) = self.chats.get_mut(&live.pane)
                    && now_ms.saturating_sub(recent.last_ms) < STEP_INTERVAL_MS
                {
                    recent.pending = Some(live);
                    return None;
                }
            }
        }
        self.chats.insert(
            live.pane.clone(),
            Recent {
                last_ms: now_ms,
                pending: None,
            },
        );
        Some(live)
    }

    /// The held updates whose interval has ended by `now_ms`, to send now.
    pub(crate) fn due(&mut self, now_ms: u64) -> Vec<Live> {
        self.chats
            .values_mut()
            .filter(|recent| now_ms.saturating_sub(recent.last_ms) >= STEP_INTERVAL_MS)
            .filter_map(|recent| {
                let live = recent.pending.take()?;
                recent.last_ms = now_ms;
                Some(live)
            })
            .collect()
    }
}
