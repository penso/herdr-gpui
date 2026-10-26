//! The sidebar sliding out to the left when it is hidden and back in when it
//! is shown, within its own room: the terminal beside it is resized once,
//! when the sidebar is shown or once it has slid off, never as it moves. A
//! sidebar that collapses to the rail slides off before the rail takes its
//! place. With the activity bar, the bar stays and the panel beside it slides.

use crate::{
    HerdrWindow,
    motion::{self, Toggle},
};
use gpui::{Context, Window};

struct Advance {
    redraw: bool,
    moving: bool,
}

#[derive(Debug)]
pub(crate) struct SidebarSlide {
    toggle: Toggle,
    /// Set for the frame being drawn: the sidebar is hidden but still
    /// sliding off, so the window keeps its expanded layout.
    closing: bool,
    /// Whether the last frame drew it moving, so the frame after it ends
    /// lays the sidebar out settled.
    moving: bool,
    /// Unit tests lay the sidebar out where a toggle leaves it unless one
    /// asks to see it move.
    #[cfg(test)]
    pub(crate) live: bool,
}

impl Default for SidebarSlide {
    fn default() -> Self {
        let mut toggle = Toggle::default();
        toggle.set(true, motion::now(), true);
        Self {
            toggle,
            closing: false,
            moving: false,
            #[cfg(test)]
            live: false,
        }
    }
}

impl SidebarSlide {
    /// Starts the sidebar sliding toward `visible` at `now`.
    pub(crate) fn set(&mut self, visible: bool, now: std::time::Instant) {
        #[cfg(test)]
        let still = !self.live;
        #[cfg(not(test))]
        let still = false;
        self.toggle.set(visible, now, still);
    }

    /// Moves the slide on to `now`, toward `visible`, and says whether the
    /// sidebar has to redraw for it and whether it still moves.
    fn advance(&mut self, visible: bool, now: std::time::Instant) -> Advance {
        if self.toggle.is_open() != visible {
            self.toggle.set(visible, now, true);
        }
        let moving = self.toggle.moving(now);
        self.closing = moving && !self.toggle.is_open();
        // Also on the frame that settles it.
        let redraw = moving || self.moving;
        self.moving = moving;
        Advance { redraw, moving }
    }

    /// Whether the window keeps the expanded sidebar's layout for the frame
    /// being drawn although the sidebar is hidden.
    pub(crate) fn closing(&self) -> bool {
        self.closing
    }
}

impl HerdrWindow {
    /// Moves the slide on for the frame being drawn. A sidebar shown or
    /// hidden some other way than a toggle, such as startup's collapsed
    /// setting, is simply there.
    pub(crate) fn tick_sidebar_slide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let advance = self
            .sidebar_slide
            .advance(self.sidebar_visible, motion::now());
        // The sidebar is a cached view: it redraws only when told.
        if advance.redraw {
            self.sidebar_view.update(cx, |_, cx| cx.notify());
        }
        if advance.moving {
            window.request_animation_frame();
        }
    }

    /// How much of the sliding sidebar shows, `None` while it is still.
    pub(crate) fn sidebar_slide_shown(&self) -> Option<f32> {
        let now = motion::now();
        let toggle = &self.sidebar_slide.toggle;
        toggle.moving(now).then(|| toggle.shown(now))
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests;
