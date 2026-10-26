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

/// A run along the strip, from the left edge of the box the tab list
/// scrolls in.
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

/// Where a strip's tab list lies, and the clip it draws under.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct List {
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) clip: Bounds<Pixels>,
}

/// What a strip's prepaint shares with its paint and the next render.
/// Cells, because the chosen tab's bounds are known only once it is laid
/// out. Everything is placed while prepainting, before anything paints, so
/// the card reads this frame's body whether it paints before the strip or
/// after it.
#[derive(Clone, Debug, Default)]
pub(crate) struct Cells {
    /// The box the tab list scrolls in and the clip the body draws under,
    /// set as the body's canvas prepaints, ahead of the tabs.
    pub(crate) list: Rc<Cell<Option<List>>>,
    /// Where the body draws this frame, set as the chosen tab prepaints;
    /// `None` while the strip shows no chosen tab. Read at the next choice,
    /// it is where a slide starts.
    pub(crate) drawn: Rc<Cell<Option<Span>>>,
    /// The part of the body the strip shows, shoulders included, in window
    /// coordinates: the card squares off a corner the body reaches.
    pub(crate) visible: Rc<Cell<Option<(Pixels, Pixels)>>>,
    /// The corner radii the card under the strip last painted, top left,
    /// top right, bottom left, bottom right.
    #[cfg(test)]
    pub(crate) corners: Rc<Cell<Option<[f32; 4]>>>,
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
        // The strip places them again as it prepaints, if it draws a
        // chosen tab.
        body.cells.list.set(None);
        body.cells.drawn.set(None);
        body.cells.visible.set(None);
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
