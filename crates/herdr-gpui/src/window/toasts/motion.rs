//! Toasts slide in from the side of the corner they sit in and slide back
//! out when they expire or are dismissed. A toast on its way out is a copy of
//! the last one drawn: the queue has already let it go, so it takes no input
//! and keeps what it said. It keeps its place in the stack too, and the room
//! it took closes as it goes, so the toasts around it do not jump.

use crate::{
    motion::{self, ENTER, LEAVE},
    notifications::Notice,
    state::LiveState,
};
use gpui::Pixels;
use herdr_client::protocol::ToastHerdrPosition;
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::Rc,
    sync::{Arc, Mutex},
    time::Instant,
};

/// How far a toast slides in from its side of the window.
const SLIDE: f32 = 16.;

/// Which toast a card shows: its host, the connection, and its queue id.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct Key {
    pub(super) endpoint: String,
    pub(super) generation: u64,
    pub(super) id: u64,
}

/// A toast as it was drawn, kept to draw it again on its way out.
#[derive(Clone)]
pub(super) struct Drawn {
    pub(super) key: Key,
    pub(super) label: String,
    pub(super) inbox: Arc<Mutex<LiveState>>,
    pub(super) notice: Notice,
    pub(super) corner: ToastHerdrPosition,
}

/// A toast sliding out, from when it went.
pub(super) struct Leaving {
    pub(super) drawn: Drawn,
    since: Instant,
    /// How many of its corner's toasts sat before it in the column, and
    /// how many after. A top corner's stack grows from its first card, a
    /// bottom corner's from its last, so each keeps its place from its own
    /// anchored end.
    pub(super) before: usize,
    pub(super) after: usize,
    /// Its height as last laid out, the room that closes as it goes.
    pub(super) height: Option<Pixels>,
}

#[derive(Default)]
pub(crate) struct ToastMotion {
    /// The toasts the last frame drew; `None` before the first, whose toasts
    /// are simply there.
    drawn: Option<Vec<Drawn>>,
    arriving: HashMap<Key, Instant>,
    leaving: Vec<Leaving>,
    /// Each drawn toast's height as last laid out.
    pub(super) heights: Rc<RefCell<HashMap<Key, Pixels>>>,
    /// Unit tests see toasts settled unless one asks to see them move, so
    /// card checks need not wait out the motion.
    #[cfg(test)]
    pub(crate) live: bool,
}

impl ToastMotion {
    /// Takes the toasts this frame draws at `now`: new ones start sliding
    /// in, and ones that are gone start sliding out from where they were.
    pub(super) fn observe(&mut self, drawn: Vec<Drawn>, now: Instant) {
        #[cfg(test)]
        let still = !self.live;
        #[cfg(not(test))]
        let still = false;
        if let Some(last) = self.drawn.as_ref().filter(|_| !still) {
            for toast in &drawn {
                if !last.iter().any(|old| old.key == toast.key) {
                    self.arriving.insert(toast.key.clone(), now);
                    // One that comes back is no longer leaving.
                    self.leaving
                        .retain(|leaving| leaving.drawn.key != toast.key);
                }
            }
            let heights = self.heights.borrow();
            for old in last {
                if !drawn.iter().any(|toast| toast.key == old.key) {
                    let stack: Vec<_> = last
                        .iter()
                        .filter(|toast| toast.corner == old.corner)
                        .collect();
                    let before = stack
                        .iter()
                        .position(|toast| toast.key == old.key)
                        .unwrap_or_default();
                    self.leaving.push(Leaving {
                        drawn: old.clone(),
                        since: now,
                        before,
                        after: stack.len().saturating_sub(before + 1),
                        height: heights.get(&old.key).copied(),
                    });
                }
            }
        }
        self.heights
            .borrow_mut()
            .retain(|key, _| drawn.iter().any(|toast| toast.key == *key));
        self.drawn = Some(drawn);
        self.arriving
            .retain(|_, since| motion::progress(*since, now, ENTER).is_some());
        self.leaving
            .retain(|leaving| motion::progress(leaving.since, now, LEAVE).is_some());
    }

    /// How much of a drawn toast shows at `now`, from 0 to 1.
    pub(super) fn shown(&self, key: &Key, now: Instant) -> f32 {
        self.arriving
            .get(key)
            .and_then(|since| motion::progress(*since, now, ENTER))
            .unwrap_or(1.)
    }

    /// The toasts sliding out, each with how much of it is left at `now`.
    pub(super) fn leaving(&self, now: Instant) -> impl Iterator<Item = (&Leaving, f32)> {
        self.leaving.iter().filter_map(move |leaving| {
            motion::progress(leaving.since, now, LEAVE).map(|k| (leaving, 1. - k))
        })
    }

    /// Holds the toasts sliding out at the start of their way, however long
    /// the frames take.
    #[cfg(test)]
    pub(super) fn hold(&mut self, start: Instant) {
        for leaving in &mut self.leaving {
            leaving.since = start;
        }
    }

    /// Whether any toast is still sliding.
    pub(super) fn moving(&self) -> bool {
        !self.arriving.is_empty() || !self.leaving.is_empty()
    }
}

/// How far a toast `shown` of the way in sits from its place: toward the
/// left edge for a left corner, the right edge for a right one.
pub(super) fn offset(corner: ToastHerdrPosition, shown: f32) -> f32 {
    let away = (1. - shown.clamp(0., 1.)) * SLIDE;
    match corner {
        ToastHerdrPosition::TopLeft | ToastHerdrPosition::BottomLeft => -away,
        ToastHerdrPosition::TopRight | ToastHerdrPosition::BottomRight => away,
    }
}

#[cfg(test)]
mod tests;
