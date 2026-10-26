//! The menu layer coming and going. A popover fades in while it slides a few
//! pixels away from what opened it; a dialog rises into place while the
//! window dims behind it, and closing it lifts the dimming back off. Moving
//! between pages of one popup, such as Add Device's sources or a menu's
//! rename field, keeps it still: only a popup that is new to the eye moves.
//!
//! All Devices instead rises out of the sidebar footer that opened it and
//! sinks back into it when it closes. It is the one page drawn again after
//! closing: closing resets the menu's state in the same frame, and its list
//! draws from what outlives that, the devices and the anchor.

use super::Page;
use crate::{
    HerdrWindow,
    motion::{self, ENTER, LEAVE, Toggle},
};
use gpui::{AnyElement, Context, Pixels, Point, Rgba, Window, div, prelude::*, px, rgba};
use std::time::Instant;

/// How far a popover slides toward its place as it appears.
const POPOVER_SLIDE: f32 = 6.;
/// How far a dialog rises into place as it appears.
const DIALOG_RISE: f32 = 10.;

/// How dark a dialog dims the window when fully shown, out of 255.
const DIM: f32 = 176.;

/// The dimming over the window behind a dialog, `shown` of the way in.
pub(super) fn backdrop(background: u32, shown: f32) -> Rgba {
    let alpha = (DIM * shown.clamp(0., 1.)).round() as u32;
    rgba((background << 8) | alpha)
}

/// What kind of popup the window shows, as far as its motion cares.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Shown {
    /// A popover beside what opened it; one opened somewhere else is new.
    Popover(Point<Pixels>),
    /// A dialog over the dimmed window.
    Dialog,
}

#[derive(Debug, Default)]
pub(crate) struct Appear {
    /// What the last frame drew, `None` while no popup was open.
    shown: Option<Shown>,
    /// The page the last frame drew.
    page: Option<Page>,
    /// When the popup showing now began to appear.
    since: Option<Instant>,
    /// A page sinking away after it closed, from when it did.
    leaving: Option<(Page, Instant)>,
    /// All Devices' height as last laid out, which its rise travels.
    pub(super) rise_height: std::rc::Rc<std::cell::Cell<Option<Pixels>>>,
    backdrop: Toggle,
    /// Unit tests lay menus out where they settle unless one asks to see
    /// them move, so geometry checks need not wait out the motion.
    #[cfg(test)]
    pub(crate) live: bool,
}

impl Appear {
    fn observe(&mut self, shown: Option<Shown>, page: Option<Page>, now: Instant) {
        let last = std::mem::replace(&mut self.page, page);
        if shown == self.shown {
            return;
        }
        #[cfg(test)]
        let still = !self.live;
        #[cfg(not(test))]
        let still = false;
        self.leaving = match (shown, last) {
            (None, Some(Page::Devices)) if !still => Some((Page::Devices, now)),
            _ => None,
        };
        self.since = shown.is_some().then_some(now).filter(|_| !still);
        self.backdrop.set(shown == Some(Shown::Dialog), now, still);
        self.shown = shown;
    }

    /// How much of the open popup shows at `now`, from 0 to 1.
    pub(super) fn panel(&self, now: Instant) -> f32 {
        self.since
            .and_then(|since| motion::progress(since, now, ENTER))
            .unwrap_or(1.)
    }

    /// How far the open popup still has to go: a popover toward its place
    /// beside its anchor, a dialog up into the middle of the window.
    pub(super) fn offset(&self, dims: bool, now: Instant) -> Pixels {
        let distance = if dims { DIALOG_RISE } else { POPOVER_SLIDE };
        px((1. - self.panel(now)) * distance)
    }

    /// How much of the dimming behind a dialog shows at `now`, from 0 to 1.
    /// It keeps fading for a moment after the dialog has gone.
    pub(super) fn backdrop(&self, now: Instant) -> f32 {
        self.backdrop.shown(now)
    }

    /// The page sinking away after it closed, and how much of it is left.
    pub(super) fn leaving(&self, now: Instant) -> Option<(Page, f32)> {
        let (page, since) = self.leaving?;
        motion::progress(since, now, LEAVE).map(|k| (page, 1. - k))
    }

    fn moving(&self, now: Instant) -> bool {
        self.panel(now) < 1. || self.backdrop.moving(now) || self.leaving(now).is_some()
    }
}

impl HerdrWindow {
    /// Takes the popup this frame draws, and keeps frames coming while it,
    /// or the dimming a closed dialog leaves, still moves.
    pub(crate) fn tick_menu_appear(&mut self, window: &mut Window) {
        let now = motion::now();
        // The application menu bar's menus draw their own way.
        let shown = self
            .menu
            .page
            .filter(|page| *page != Page::Application)
            .map(|_| match self.menu_dims() {
                true => Shown::Dialog,
                false => Shown::Popover(self.menu.anchor),
            });
        self.menu.appear.observe(shown, self.menu.page, now);
        if self.menu.appear.moving(now) {
            window.request_animation_frame();
        }
    }
}

impl HerdrWindow {
    /// What a popup that just closed leaves while it goes: a dialog's
    /// dimming lifting, or All Devices sinking into the footer. Neither takes
    /// input: the window beneath is already the one being used.
    pub(crate) fn render_menu_leaving(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if self.menu.page.is_some() {
            return Vec::new();
        }
        let now = motion::now();
        let shown = self.menu.appear.backdrop(now);
        let fade = (shown > 0.).then(|| {
            div()
                .id("menu-fade")
                .debug_selector(|| "menu-fade".into())
                .absolute()
                .inset_0()
                .bg(backdrop(self.theme.background, shown))
                .into_any_element()
        });
        let page = self.menu.appear.leaving(now).map(|(page, left)| {
            self.render_menu_layer(page, Some(left), window, cx)
                .into_any_element()
        });
        fade.into_iter().chain(page).collect()
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests;
