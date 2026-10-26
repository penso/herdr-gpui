//! The connected body of a layout that draws one
//! ([`crate::tab_layout::TabLayout::connected`]): where each strip's body
//! drew and where it is sliding from, so the chosen tab's surface glides to
//! a newly chosen tab rather than jumping there.
use super::{GroupId, Pick};
use gpui::{Bounds, Pixels};
use std::{
    cell::Cell,
    collections::HashMap,
    rc::Rc,
    time::{Duration, Instant},
};

/// How long the body takes to reach a newly chosen tab.
pub(crate) const SLIDE: Duration = Duration::from_millis(240);

/// A run along the strip, from the tab list's own left edge, so it holds
/// still while the list scrolls.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Span {
    pub(crate) left: f32,
    pub(crate) width: f32,
}

impl Span {
    /// `k` of the way from `self` to `to`.
    pub(crate) fn lerp(self, to: Self, k: f32) -> Self {
        Self {
            left: self.left + (to.left - self.left) * k,
            width: self.width + (to.width - self.width) * k,
        }
    }

    /// Where the body draws when it is heading for `self`, sliding from
    /// `from`, at `now`.
    pub(crate) fn from(self, from: Option<(Span, Instant)>, now: Instant) -> Self {
        let Some((start, since)) = from else {
            return self;
        };
        crate::motion::progress(since, now, crate::motion::duration(SLIDE))
            .map_or(self, |k| start.lerp(self, k))
    }
}

/// What a strip's paint shares with its render. Cells, because the chosen
/// tab's bounds are known only once it is laid out, and the card under the
/// strip reads where the body landed in the same frame.
#[derive(Clone, Debug, Default)]
pub(crate) struct Cells {
    /// The chosen tab's bounds this frame, set as it prepaints; `None`
    /// while the strip shows no chosen tab.
    pub(crate) target: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Where the body drew last.
    pub(crate) drawn: Rc<Cell<Option<Span>>>,
    /// The part of the body the strip shows, shoulders included, in window
    /// coordinates: the card squares off a corner the body reaches.
    pub(crate) visible: Rc<Cell<Option<(Pixels, Pixels)>>>,
}

#[derive(Debug, Default)]
struct Body {
    pick: Option<Pick>,
    from: Option<(Span, Instant)>,
    cells: Cells,
}

/// One frame of a strip's body.
#[derive(Clone, Debug)]
pub(crate) struct Frame {
    pub(crate) cells: Cells,
    /// Where a slide started and when, while it runs.
    pub(crate) from: Option<(Span, Instant)>,
}

impl Frame {
    /// Whether the body is still sliding at `now`, so the strip draws again.
    pub(crate) fn moving(&self, now: Instant) -> bool {
        self.from.is_some_and(|(_, since)| {
            crate::motion::progress(since, now, crate::motion::duration(SLIDE)).is_some()
        })
    }
}

/// Every strip's body.
#[derive(Debug, Default)]
pub(crate) struct TabBodies {
    bodies: HashMap<GroupId, Body>,
}

impl TabBodies {
    /// `group`'s body for a frame where `pick` is chosen at `now`. A new
    /// choice slides from wherever the body drew last; the first one, or one
    /// after the strip showed nothing chosen, is simply there.
    pub(crate) fn frame(&mut self, group: GroupId, pick: Option<&Pick>, now: Instant) -> Frame {
        let body = self.bodies.entry(group).or_default();
        if body.pick.as_ref() != pick {
            body.from = body
                .pick
                .as_ref()
                .and(body.cells.drawn.get())
                .map(|span| (span, now));
            body.pick = pick.cloned();
        }
        let frame = Frame {
            cells: body.cells.clone(),
            from: body.from,
        };
        // A slide that has arrived is forgotten.
        let frame = if frame.moving(now) {
            frame
        } else {
            body.from = None;
            Frame {
                from: None,
                ..frame
            }
        };
        // The chosen tab sets it again as it prepaints, if it is drawn.
        body.cells.target.set(None);
        frame
    }

    /// What `group`'s strip shared this frame, for the card under it.
    pub(crate) fn cells(&self, group: GroupId) -> Option<Cells> {
        self.bodies.get(&group).map(|body| body.cells.clone())
    }

    pub(crate) fn retain(&mut self, live: impl Fn(GroupId) -> bool) {
        self.bodies.retain(|group, _| live(*group));
    }
}

#[cfg(test)]
mod tests;
