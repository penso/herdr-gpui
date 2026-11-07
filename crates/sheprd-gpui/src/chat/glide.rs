//! Following a streaming reply by gliding to the end of the chat instead of
//! jumping there with each chunk.
//!
//! The list itself never follows its tail: aligned to its top, it keeps
//! its top where it is while entries grow beneath it, even at the end,
//! where a list aligned to its bottom would stick to the end and jump.
//! Each frame [`Glide`] moves the list toward its end on a critically
//! damped spring, which keeps its speed when more text lands, so a stream
//! of chunks reads as one steady glide rather than a dash per chunk, and
//! settles at the end without passing it, and [`Glide::watch`] asks for
//! that frame once a layout leaves the list behind its end, whether an
//! entry grew or the list itself got shorter. With `[chat] glide` off, the
//! list is put at its end before each layout instead, so new text shows
//! in the frame it arrives in. Either way, scrolling up stops the
//! following; scrolling back to the end starts it again, as does the
//! button [`Glide::away_shown`] shows while the list is left behind.
//!
//! Whether a scroll left the end is only decided after layout, in
//! [`Glide::watch`]: a row redrawn by a chunk counts no height until it is
//! laid out, so before layout a tall reply makes any place look like the
//! end, and a reader scrolling up would be pulled back down.

use crate::motion;
use gpui::{ListOffset, ListState, canvas, prelude::*, px};
use std::{cell::Cell, ops::Range, rc::Rc, time::Instant};

/// How long the spring takes to settle, roughly: a few new lines arrive in
/// about this long, and text streaming steadily is followed this far behind.
const SMOOTH_SECONDS: f32 = 0.12;
/// The longest frame gap counted, so a stall does not land in one jump.
const MAX_FRAME_SECONDS: f32 = 0.05;
/// A frame gap assumed for the first step of a glide.
const FRAME_SECONDS: f32 = 1. / 60.;
/// Distances below this count as being at the end.
const AT_END: f32 = 1.;
/// The least a step moves, so the end comes in finite time.
const MIN_STEP: f32 = 0.5;
/// More than this many viewports behind is closed at once, down to it, so
/// a long burst of output does not take seconds to scroll past.
const MAX_BEHIND: f32 = 0.5;

/// Whether the list's top moved to another row, or to or from a row's start.
fn crossed(before: ListOffset, after: ListOffset) -> bool {
    before.item_ix != after.item_ix
        || (before.offset_in_item > px(0.)) != (after.offset_in_item > px(0.))
}

#[derive(Debug)]
pub(crate) struct Glide {
    /// Shared with the frame's [`Self::watch`], which reads it after layout.
    following: Rc<Cell<bool>>,
    /// Set by the list when the user scrolls it, by wheel or trackpad. Only
    /// the user's scrolling decides following: the list moving its own top,
    /// as it does when the end comes up short, must not stop it.
    scrolled: Rc<Cell<bool>>,
    last_step: Option<Instant>,
    /// The spring's speed, in pixels a second.
    speed: f32,
    /// The button back to the end, open while the list stays behind it.
    /// Shared with [`Self::watch`], which alone knows the list's real end.
    away: Rc<Cell<motion::Toggle>>,
    /// Whether output landed below since the button last closed.
    waiting: Rc<Cell<bool>>,
    /// Where the last layout left the list's top. Render cannot ask the
    /// list: until layout it answers with where it was told to go, which
    /// while following is past the end, however short the conversation.
    top: Rc<Cell<ListOffset>>,
}

impl Glide {
    /// Follows `list`'s end, watching it for the user's scrolling.
    pub(crate) fn new(list: &ListState) -> Self {
        let scrolled = Rc::new(Cell::new(false));
        list.set_scroll_handler({
            let scrolled = scrolled.clone();
            move |_, _, _| scrolled.set(true)
        });
        Self {
            following: Rc::new(Cell::new(true)),
            scrolled,
            last_step: None,
            speed: 0.,
            away: Rc::default(),
            waiting: Rc::default(),
            top: Rc::default(),
        }
    }

    /// Back to following, with `list` at its end: it started over, or the
    /// user asked to go there.
    pub(crate) fn reset(&mut self, list: &ListState) {
        self.following.set(true);
        self.scrolled.set(false);
        self.last_step = None;
        self.speed = 0.;
        list.scroll_to(ListOffset {
            item_ix: list.item_count(),
            offset_in_item: px(0.),
        });
    }

    /// A keyboard or test scroll stops following unless it ends at the end.
    /// Wheel and trackpad events tell the list directly.
    pub(crate) fn user_scrolled(&self) {
        self.scrolled.set(true);
    }

    /// Whether the user left the list, or is leaving it, above its end.
    pub(crate) fn stopped(&self) -> bool {
        self.scrolled.get() || !self.following.get()
    }

    /// Where the last layout left the list's top.
    pub(crate) fn top(&self) -> ListOffset {
        self.top.get()
    }

    /// New output landed at the end of the list.
    pub(crate) fn note_output(&self) {
        self.waiting.set(true);
    }

    /// Whether output is waiting below a list left above its end.
    pub(crate) fn waiting(&self) -> bool {
        self.waiting.get()
    }

    /// An empty element to place after `list` in the frame: once the list
    /// is laid out, with heights the frame's render could not know yet, it
    /// settles what the user's scrolling meant and whether the button back
    /// to the end shows, and asks for another frame if the list is behind
    /// its end and following or the button is moving. A list `held` in
    /// place, as a selection holds it, follows nothing until it is let go,
    /// but is still away from its end.
    pub(crate) fn watch(&self, list: &ListState, held: bool) -> impl IntoElement + use<> {
        let following = self.following.clone();
        let scrolled = self.scrolled.clone();
        let away = self.away.clone();
        let waiting = self.waiting.clone();
        let top = self.top.clone();
        let list = list.clone();
        canvas(
            move |_, window, _| {
                let settled = list.logical_scroll_top();
                // What reads the top only cares which row it is in, and
                // whether that row starts there.
                if crossed(top.replace(settled), settled) {
                    window.request_animation_frame();
                }
                let left = behind(&list);
                if !held {
                    settle(&following, &scrolled, left);
                }
                let stopped = held || scrolled.get() || !following.get();
                let now = motion::now();
                let mut button = away.get();
                button.set(stopped && left > AT_END, now, false);
                away.set(button);
                if button.moving(now) {
                    window.request_animation_frame();
                } else if !button.is_open() {
                    waiting.set(false);
                }
                if !held && following.get() && left > MIN_STEP {
                    window.request_animation_frame();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_0()
    }

    /// How much of the button back to the end shows at `now`, if any of it
    /// does.
    pub(crate) fn away_shown(&self, now: Instant) -> Option<f32> {
        let button = self.away.get();
        let shown = button.shown(now);
        (button.is_open() || shown > 0.).then_some(shown)
    }

    /// Scrolls `list` one frame's step toward its end, or with `smooth`
    /// off all the way there, and says whether to draw another frame.
    pub(crate) fn tick(&mut self, list: &ListState, now: Instant, smooth: bool) -> bool {
        // The user's scroll stands until this frame's layout judges it.
        if self.scrolled.get() {
            self.last_step = None;
            self.speed = 0.;
            return false;
        }
        let top = -f32::from(list.scroll_px_offset_for_scrollbar().y);
        let end = top + behind(list);
        if !smooth {
            self.last_step = None;
            self.speed = 0.;
            // At the end before this frame's layout, which then lays out
            // whatever arrived with the end in view.
            if self.following.get() {
                list.scroll_to(ListOffset {
                    item_ix: list.item_count(),
                    offset_in_item: px(0.),
                });
            }
            return false;
        }
        let viewport = f32::from(list.viewport_bounds().size.height);
        if let Some(distance) = self.step(top, end, viewport, now) {
            list.scroll_by(px(distance));
        }
        self.last_step.is_some()
    }

    /// How far to scroll this frame, from the top's offset, the offset at
    /// the end, and the viewport's height, all in pixels.
    fn step(&mut self, top: f32, end: f32, viewport: f32, now: Instant) -> Option<f32> {
        let remaining = end - top;
        if !self.following.get() || remaining <= MIN_STEP {
            self.last_step = None;
            self.speed = 0.;
            return None;
        }
        let elapsed = self.last_step.map_or(FRAME_SECONDS, |last| {
            now.saturating_duration_since(last)
                .as_secs_f32()
                .min(MAX_FRAME_SECONDS)
        });
        self.last_step = Some(now);
        let jump = (remaining - viewport * MAX_BEHIND).max(0.);
        let left = remaining - jump;
        // A critically damped spring toward the end, stepped exactly for
        // `elapsed`: from `left` away at `speed`, it is
        // `(left - (speed - omega * left) * t) * e^(-omega * t)` away.
        // Its speed carries across a moved end, so it never lurches.
        let omega = 2. / SMOOTH_SECONDS;
        let decay = (-omega * elapsed).exp();
        let lead = self.speed - omega * left;
        let mut rest = (left - lead * elapsed) * decay;
        self.speed = (self.speed - omega * lead * elapsed) * decay;
        if rest < 0. {
            // It would pass the end: stop there instead.
            rest = 0.;
            self.speed = 0.;
        }
        let distance = (jump + (left - rest).max(MIN_STEP)).min(remaining);
        Some(distance)
    }
}

/// Settles a user's scroll, `remaining` pixels from the end as laid out: it
/// leaves the list following only at its end.
fn settle(following: &Cell<bool>, scrolled: &Cell<bool>, remaining: f32) {
    if scrolled.take() {
        following.set(remaining <= AT_END);
    }
}

/// How far `list`'s top is above its end, in pixels, as of its last layout.
fn behind(list: &ListState) -> f32 {
    f32::from(list.max_offset_for_scrollbar().y + list.scroll_px_offset_for_scrollbar().y)
}

/// `list.splice`, keeping the list's top where it was when the entry it
/// falls in is redrawn in place, such as a reply that grew: the list
/// itself moves the top to that entry's start.
pub(crate) fn splice_in_place(list: &ListState, range: Range<usize>, count: usize) {
    let top = list.logical_scroll_top();
    list.splice(range.clone(), count);
    if range.len() == count && range.contains(&top.item_ix) {
        list.scroll_to(top);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
