//! Zen mode: the window keeps only the group in use, with no header, sidebar,
//! tab strips, status bar, banner, or notices around it. It is this window's
//! own presentation for the session, never a Herdr setting, so the daemon and
//! its other clients see nothing change.
//!
//! Render reads [`HerdrWindow::zen_layout`] instead of zen mode rewriting the
//! sidebar or status bar flags, so leaving it shows exactly what was there
//! before, including toggles made while the chrome was hidden.

use super::{Flash, HerdrWindow};
use crate::{
    browser::{Pick, Slot},
    config::ZenWidth,
    controls::Command,
};
use chrome::{Chrome, Shown};
use gpui::{AnyElement, Context, Window, prelude::*, *};

/// Whether this window is in zen mode, and what leaving it must undo.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum Zen {
    #[default]
    Off,
    On {
        /// Zen mode put the window in full screen, so leaving it takes the
        /// window back out. One already full screen stays that way.
        entered_fullscreen: bool,
        /// The terminal font size before `[zen] font_size` replaced it.
        restore_font: Option<f32>,
    },
}

/// Zen mode and the chrome sliding away or back as it starts and ends.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ZenState {
    pub(crate) mode: Zen,
    /// How much of the sidebar and status bar shows as they slide.
    chrome: Chrome,
    /// Off in tests, which draw the settled window unless they ask.
    animate: bool,
    /// Zen mode has begun but the chrome is still sliding off, so the
    /// window keeps its full layout; see [`HerdrWindow::zen_layout`].
    arriving: bool,
    /// Slides stay where they are, for tests to look at.
    held: bool,
}

impl Default for ZenState {
    fn default() -> Self {
        Self {
            mode: Zen::Off,
            chrome: Chrome::default(),
            animate: !cfg!(test),
            arriving: false,
            held: false,
        }
    }
}

#[cfg(test)]
impl ZenState {
    /// Makes later toggles slide, with every slide held at its start until
    /// [`Self::finish_slide`].
    pub(crate) fn hold_slides(&mut self) {
        self.animate = true;
        self.held = true;
    }

    /// Ends a held slide where it was going; the next frame drawn settles.
    pub(crate) fn finish_slide(&mut self) {
        match self.mode {
            Zen::On { .. } => self.chrome.hide(false),
            Zen::Off => self.chrome.show(false),
        }
    }
}

/// `element`, `width` wide, in a box its own size that clips it, drawn
/// further left as `shown` falls from 1 to 0.
pub(crate) fn slide_left(element: impl IntoElement, width: f32, shown: f32) -> Div {
    div()
        .relative()
        .flex_none()
        .w(px(width))
        .h_full()
        .overflow_hidden()
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .left(px(-(1. - shown) * width))
                .w(px(width))
                .flex()
                .child(element),
        )
}

/// `element`, `height` tall, in a box its own size that clips it, drawn
/// further down as `shown` falls from 1 to 0.
fn slide_down(element: impl IntoElement, height: f32, shown: f32) -> Div {
    div()
        .relative()
        .flex_none()
        .w_full()
        .h(px(height))
        .overflow_hidden()
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px((1. - shown) * height))
                .h(px(height))
                .flex()
                .flex_col()
                .child(element),
        )
}

impl HerdrWindow {
    pub(crate) fn zen_active(&self) -> bool {
        matches!(self.zen.mode, Zen::On { .. })
    }

    /// Whether the window is laid out for zen mode: from when the chrome has
    /// slid off on the way in until zen mode ends.
    pub(crate) fn zen_layout(&self) -> bool {
        self.zen_active() && !self.zen.arriving
    }

    /// How much of the chrome shows while it slides, `None` once settled.
    fn chrome_slide(&self) -> Option<Shown> {
        let (shown, moving) = self.zen.chrome.at();
        moving.then_some(shown)
    }

    /// Moves the sliding chrome on for the frame being drawn, and takes zen
    /// mode's layout once it has slid off.
    pub(super) fn tick_chrome(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.zen.held {
            self.zen.chrome.tick(crate::motion::now());
        }
        self.settle_zen(window, cx);
    }

    /// Whether the window draws another frame for the sliding chrome.
    pub(super) fn chrome_sliding(&self) -> bool {
        self.chrome_slide().is_some()
    }

    /// The sidebar's column, `width` wide, sliding out to the left on the
    /// way in and back on the way out, always within its own room: the
    /// terminal beside it keeps its size while it moves.
    pub(super) fn sliding_sidebar(&self, column: AnyElement, width: f32) -> AnyElement {
        // With the activity bar, a toggle slides only the panel beside it.
        let toggled = self
            .sidebar_slide_shown()
            .filter(|_| !self.activity_bar_leads());
        match self.chrome_slide().map(|shown| shown.sidebar).or(toggled) {
            Some(shown) => slide_left(column, width, shown).into_any_element(),
            None => column,
        }
    }

    /// The status bar outside zen mode's layout, sliding down out of its
    /// room on the way in and back up into it on the way out.
    pub(super) fn zen_status_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.status_bar_visible || self.zen_layout() {
            return None;
        }
        let bar = self.render_status_bar(cx);
        Some(match self.chrome_slide() {
            Some(shown) => slide_down(bar, self.status_bar_height(), shown.bar).into_any_element(),
            None => bar.into_any_element(),
        })
    }

    /// Lays the window out for zen mode once the chrome has slid off, with
    /// `[zen]`'s full screen and font size: each resizes the terminal, which
    /// would jolt the text under a slide still running.
    fn settle_zen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.zen.arriving || self.chrome_slide().is_some() {
            return;
        }
        self.zen.arriving = false;
        let entered_fullscreen = self.config.zen.fullscreen && !window.is_fullscreen();
        if entered_fullscreen {
            window.toggle_fullscreen();
        }
        let restore_font = self.config.zen.font_size.map(|size| {
            let previous = self.config.terminal.size;
            self.set_terminal_font_size(size, cx);
            previous
        });
        self.zen.mode = Zen::On {
            entered_fullscreen,
            restore_font,
        };
        // The sidebar stays usable while it slides, so its search may have
        // taken the keyboard since the toggle; it is hidden now. Anything
        // else, such as a menu opened meanwhile, keeps it.
        if self.sidebar_search.focus(cx).contains_focused(window, cx) {
            self.zen_focus(window, cx);
        }
        cx.notify();
    }

    /// Window chrome only, so it runs while the daemon is away. Entering is
    /// leaving played backwards: the window keeps its layout while the
    /// status bar and then the sidebar slide out of their room, and only
    /// then takes zen mode's. Leaving takes the full layout at once and
    /// slides both back into the room it makes for them.
    pub(super) fn toggle_zen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let animate = self.zen.animate;
        match self.zen.mode {
            Zen::Off => {
                let key = self.keymap().primary(Command::ToggleZenMode);
                self.show_flash(
                    Flash::success(match key {
                        "" => "Zen mode: run Toggle Zen Mode to leave".to_owned(),
                        key => format!("Zen mode: {key} to leave"),
                    }),
                    cx,
                );
                self.zen.mode = Zen::On {
                    entered_fullscreen: false,
                    restore_font: None,
                };
                self.zen.arriving = true;
                self.zen.chrome.hide(animate);
                self.settle_zen(window, cx);
            }
            Zen::On {
                entered_fullscreen,
                restore_font,
            } => {
                // Nothing to undo when it ends before the chrome had gone.
                self.zen.arriving = false;
                // As zen-mode.nvim does, whatever size was set meanwhile.
                if let Some(size) = restore_font {
                    self.set_terminal_font_size(size, cx);
                }
                // Left alone when the user already took the window out.
                if entered_fullscreen && window.is_fullscreen() {
                    window.toggle_fullscreen();
                }
                self.zen.mode = Zen::Off;
                self.zen.chrome.show(animate);
            }
        }
        self.zen_focus(window, cx);
        cx.notify();
    }

    /// Hands the keyboard to what the group in use draws. A code, review,
    /// or devices tab tracks its own handle and keeps the keyboard where it
    /// is within it; anything else, such as the sidebar's search, may be
    /// hidden by zen mode, so the window's handle takes the keyboard to the
    /// terminal, or to the page or stand-in drawn in its place.
    fn zen_focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        let own = self
            .active_group()
            .and_then(|group| match self.group_pick(group) {
                Some(Pick::Page(id)) => Some(id),
                _ => None,
            })
            .and_then(|id| {
                let code = self.code_views.get(&id).map(|view| &view.focus);
                let review = || self.reviews.get(&id).map(|review| review.focus());
                let devices = || self.devices_overview.pages.get(&id).map(|page| &page.focus);
                code.or_else(review).or_else(devices)
            });
        match own {
            Some(focus) if focus.contains_focused(window, cx) => {}
            Some(focus) => window.focus(focus, cx),
            None => window.focus(&self.focus, cx),
        }
    }

    /// The groups the window draws, left to right: all of them, or in zen
    /// mode only the one in use. The others keep their tabs and layout, and
    /// come back when zen mode ends.
    pub(crate) fn drawn_group_slots(&self) -> Vec<Slot> {
        let slots = self.group_slots();
        if !self.zen_layout() {
            return slots;
        }
        let active = self.active_group();
        let kept = slots
            .iter()
            .copied()
            .find(|slot| Some(slot.id) == active)
            .or_else(|| slots.first().copied());
        kept.into_iter().collect()
    }

    /// The groups in zen mode's centred column, with the terminal's own
    /// ground either side. Narrowing the terminal's box is what resizes it,
    /// through the same canvas bounds every other resize reads.
    pub(super) fn zen_column(&self, groups: AnyElement) -> AnyElement {
        let Some(width) = self.config.zen.width.filter(|_| self.zen_layout()) else {
            return groups;
        };
        let column = div()
            .debug_selector(|| "zen-column".into())
            .flex()
            .flex_col()
            .h_full()
            .min_w_0()
            .max_w_full()
            .child(groups);
        let column = match width {
            // Half a cell spare, so rounding never costs the last column,
            // and room for `[terminal]`'s padding beside the cells.
            ZenWidth::Columns(columns) => {
                let padding = self.config.terminal_padding.x;
                column.w(px((f32::from(columns) + 0.5) * self.cell_width
                    + padding.start()
                    + padding.end()))
            }
            ZenWidth::Percent(share) => column.w(relative(f32::from(share) / 100.)),
        };
        self.with_ground(
            div()
                .flex()
                .flex_1()
                .min_h_0()
                .min_w_0()
                .justify_center()
                .child(column),
            self.fill().ground(),
        )
    }
}

mod chrome;

#[cfg(test)]
mod tests;
