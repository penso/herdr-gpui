//! What a resizable edge shows: nothing at rest, then a short pill on the
//! edge that grows as the pointer hovers, presses, and drags it, so the
//! edge in hand stands out without a line on every edge all the time.
use crate::motion;
use gpui::{prelude::*, *};
use std::time::Instant;

/// The pill's thickness across the edge.
pub(crate) const THICKNESS: f32 = 3.;
/// The pill's color, the accent edges have always shown under the pointer.
const COLOR: u32 = 0x78a9ff;

/// How engaged the pointer is with an edge.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Level {
    #[default]
    Idle,
    Hovered,
    Pressed,
    Dragging,
}

impl Level {
    /// The pill's length along the edge and its opacity at this level.
    fn look(self) -> (f32, f32) {
        match self {
            Self::Idle => (0., 0.),
            Self::Hovered => (20., 0.35),
            Self::Pressed => (28., 0.6),
            Self::Dragging => (44., 0.9),
        }
    }
}

/// The pill, easing from where it was toward its level's look.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct Indicator {
    level: Level,
    /// The look it started from when the level last changed.
    from: (f32, f32),
    /// When the level last changed; `None` before, so it starts still.
    since: Option<Instant>,
}

impl Indicator {
    /// Moves toward `level` from wherever it is at `now`, so a change
    /// mid-way turns back smoothly rather than jumping to an end.
    pub(super) fn set(&mut self, level: Level, now: Instant) {
        if level == self.level {
            return;
        }
        self.from = self.look(now);
        self.level = level;
        self.since = Some(now);
    }

    /// The pill's length and opacity at `now`.
    pub(super) fn look(&self, now: Instant) -> (f32, f32) {
        let (length, opacity) = self.level.look();
        let Some(k) = self.progress(now) else {
            return (length, opacity);
        };
        let (from_length, from_opacity) = self.from;
        (
            from_length + (length - from_length) * k,
            from_opacity + (opacity - from_opacity) * k,
        )
    }

    /// Whether it is still moving at `now`.
    pub(super) fn moving(&self, now: Instant) -> bool {
        self.progress(now).is_some()
    }

    fn progress(&self, now: Instant) -> Option<f32> {
        self.since
            .and_then(|since| motion::progress(since, now, motion::ENTER))
    }
}

/// The pointer's hold on one edge, kept across frames.
#[derive(Default)]
struct Hold {
    /// The pieces of the edge under the pointer, one bit each, so an edge
    /// drawn in several pieces stays hovered whichever order the pointer's
    /// leaving one and entering the next arrive in.
    hovered: u8,
    pressed: bool,
    indicator: Indicator,
}

impl Hold {
    /// How engaged the pointer is, given whether this edge's drag is under
    /// way: a GPUI drag counts only once this edge was pressed, as it may be
    /// another edge's.
    fn level(&self, gpui_drag: bool, own_drag: bool) -> Level {
        match (
            own_drag || (self.pressed && gpui_drag),
            self.pressed,
            self.hovered != 0,
        ) {
            (true, _, _) => Level::Dragging,
            (false, true, _) => Level::Pressed,
            (false, false, true) => Level::Hovered,
            (false, false, false) => Level::Idle,
        }
    }

    fn hover(&mut self, piece: u8, over: bool) {
        let bit = 1 << piece;
        match over {
            true => self.hovered |= bit,
            false => self.hovered &= !bit,
        }
    }
}

/// The pointer's hold on an edge, which every piece of the edge shares.
#[derive(Clone)]
pub(crate) struct GripHold {
    hold: Entity<Hold>,
    /// The key's name; the pill's debug selector is it with `-pill`.
    name: SharedString,
}

impl GripHold {
    /// The hold keyed `id` in the view being drawn, kept while it is drawn.
    pub(crate) fn keyed(id: impl Into<ElementId>, window: &mut Window, cx: &mut App) -> Self {
        let id = id.into();
        let name = id.to_string().into();
        Self {
            hold: window.use_keyed_state(id, cx, |_, _| Hold::default()),
            name,
        }
    }

    /// Makes `band` the `piece`th piece of the edge (below 8): hovering,
    /// pressing, and releasing it count for the whole edge.
    pub(crate) fn listen(&self, piece: u8, band: Stateful<Div>) -> Stateful<Div> {
        let hold = &self.hold;
        // A hover ends unreported while a drag is under way, so a release
        // settles whether the pointer is still on the piece. Both listen in
        // the capture phase, which an owner's drag that stops the release
        // still lets through first.
        let release = |over: bool| {
            let hold = hold.clone();
            move |_: &MouseUpEvent, _: &mut Window, cx: &mut App| {
                hold.update(cx, |hold, cx| {
                    hold.pressed = false;
                    hold.hover(piece, over);
                    cx.notify();
                });
            }
        };
        let pressed = hold.clone();
        let hovered = hold.clone();
        band.on_hover(move |over, _, cx| {
            hovered.update(cx, |hold, cx| {
                hold.hover(piece, *over);
                cx.notify();
            });
        })
        // The capture phase sees the press however the band's own
        // listeners stop it.
        .capture_any_mouse_down(move |event, _, cx| {
            if event.button == MouseButton::Left {
                pressed.update(cx, |hold, cx| {
                    hold.pressed = true;
                    cx.notify();
                });
            }
        })
        .capture_any_mouse_up({
            let release = release(true);
            move |event, window, cx| {
                if event.button == MouseButton::Left {
                    release(event, window, cx);
                }
            }
        })
        .on_mouse_up_out(MouseButton::Left, release(false))
    }

    /// The pill as it shows now along `axis`, or `None` while it is hidden;
    /// `own_drag` reports a drag the owner tracks itself. Asks for frames
    /// while it moves.
    pub(crate) fn pill(
        &self,
        axis: Axis,
        own_drag: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Div> {
        let now = motion::now();
        // Unit tests see the pill where it settles, as `motion::pop_in`'s.
        let at = if cfg!(test) { now + motion::ENTER } else { now };
        let (length, opacity, moving) = self.hold.update(cx, |hold, cx| {
            let level = hold.level(cx.has_active_drag(), own_drag);
            hold.indicator.set(level, now);
            let (length, opacity) = hold.indicator.look(at);
            (length, opacity, hold.indicator.moving(at))
        });
        if moving {
            window.request_animation_frame();
        }
        let name = self.name.clone();
        (length > 0.5).then(|| {
            let pill = div()
                .debug_selector(move || format!("{name}-pill"))
                .flex_none()
                .rounded_full()
                .bg(rgb(COLOR))
                .opacity(opacity.clamp(0., 1.));
            match axis {
                Axis::Horizontal => pill.w(px(THICKNESS)).h(px(length)),
                Axis::Vertical => pill.h(px(THICKNESS)).w(px(length)),
            }
        })
    }
}

/// A grabbable band that shows the pill. `axis` is the direction the band
/// resizes: horizontal for an edge between side-by-side panes. The band
/// keeps its own id, cursor, and drag; it needs an id for the pill's state.
#[derive(IntoElement)]
pub(crate) struct Grip {
    axis: Axis,
    band: Stateful<Div>,
    dragging: bool,
}

impl Grip {
    pub(crate) fn new(axis: Axis, band: Stateful<Div>) -> Self {
        Self {
            axis,
            band,
            dragging: false,
        }
    }

    /// Marks the band dragged by a drag its owner tracks itself rather
    /// than through GPUI's `on_drag`, as the sidebar's edges are.
    pub(crate) fn dragging(mut self, dragging: bool) -> Self {
        self.dragging = dragging;
        self
    }
}

impl RenderOnce for Grip {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            axis,
            mut band,
            dragging,
        } = self;
        let Some(id) = band.interactivity().element_id.clone() else {
            return band;
        };
        let hold = GripHold::keyed(id, window, cx);
        let pill = hold.pill(axis, dragging, window, cx);
        hold.listen(0, band)
            .flex()
            .items_center()
            .justify_center()
            .children(pill)
    }
}

#[cfg(test)]
mod tests;
