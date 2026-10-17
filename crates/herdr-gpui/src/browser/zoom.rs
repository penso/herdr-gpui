//! Page zoom: how large a browser tab draws its page, as a web browser's
//! View menu zooms one. Each tab keeps its own, saved with it, and every
//! window's page for the tab follows it as the window presents its pages.
//! The font size commands zoom the focused page rather than the terminal
//! font, so the same keys serve both.
use super::{Location, Shown, Store, Tab, TabId, view::store};
use crate::{HerdrWindow, control::ZoomChange, controls::Command};
use gpui::{App, Context, Div, Stateful, div, prelude::*, px, rgb};
use serde::{Deserialize, Serialize};

/// The zooms a step lands on, in percent, as web browsers offer them.
const STEPS: &[u16] = &[50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300];
const ACTUAL: u16 = 100;
pub(crate) const MIN_PERCENT: u16 = 50;
pub(crate) const MAX_PERCENT: u16 = 300;

/// A page's zoom in percent, from `MIN_PERCENT` to `MAX_PERCENT`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "u16", into = "u16")]
pub(crate) struct Zoom(u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("Page zoom must be between {MIN_PERCENT}% and {MAX_PERCENT}%, not {0}%")]
pub(crate) struct OutOfRange(pub u16);

impl Default for Zoom {
    fn default() -> Self {
        Self(ACTUAL)
    }
}

impl Zoom {
    pub(crate) fn percent(self) -> u16 {
        self.0
    }

    /// The scale a web view takes, where 1 draws the page at its own size.
    #[cfg(any(target_os = "macos", windows, test))]
    pub(crate) fn factor(self) -> f64 {
        f64::from(self.0) / f64::from(ACTUAL)
    }

    /// `rect`, in CSS pixels of a page drawn at this zoom, in its web view's
    /// own points, which a snapshot of the view is asked for. They match only
    /// at 100%: at 150% a CSS pixel covers one and a half points.
    #[cfg(any(target_os = "macos", test))]
    pub(crate) fn view_rect(self, rect: super::annotate::Rect) -> super::annotate::Rect {
        let factor = self.factor();
        super::annotate::Rect {
            x: rect.x * factor,
            y: rect.y * factor,
            width: rect.width * factor,
            height: rect.height * factor,
        }
    }

    /// Whether the page draws at its own size.
    pub(crate) fn is_actual(&self) -> bool {
        self.0 == ACTUAL
    }

    /// The zoom `change` leads to from this one. A step from between two
    /// steps lands on the nearer one in its direction, and stays at either
    /// end of the range.
    pub(crate) fn changed(self, change: ZoomChange) -> Result<Self, OutOfRange> {
        let step = |found: Option<&u16>| found.map_or(self, |step| Self(*step));
        Ok(match change {
            ZoomChange::In => step(STEPS.iter().find(|step| **step > self.0)),
            ZoomChange::Out => step(STEPS.iter().rev().find(|step| **step < self.0)),
            ZoomChange::Reset => Self::default(),
            ZoomChange::Percent { percent } => Self::try_from(percent)?,
        })
    }
}

impl TryFrom<u16> for Zoom {
    type Error = OutOfRange;

    fn try_from(percent: u16) -> Result<Self, OutOfRange> {
        if (MIN_PERCENT..=MAX_PERCENT).contains(&percent) {
            Ok(Self(percent))
        } else {
            Err(OutOfRange(percent))
        }
    }
}

impl From<Zoom> for u16 {
    fn from(zoom: Zoom) -> Self {
        zoom.0
    }
}

impl HerdrWindow {
    /// The tab the zoom keys zoom: the one whose page holds the keyboard,
    /// as a click into a page of another group leaves it, else the one the
    /// group in use shows. Only a tab a page draws zooms: a web page, a
    /// local page, or a blank tab waiting for one. Tabs the app draws
    /// itself, such as a review, follow the terminal font instead.
    fn zoomed_tab(&self, cx: &App) -> Option<Tab> {
        if !super::EMBEDDED {
            return None;
        }
        #[cfg(any(target_os = "macos", windows))]
        let typing = self.browser.pages.keyboard_page();
        #[cfg(not(any(target_os = "macos", windows)))]
        let typing = None;
        let id = match typing {
            Some(id) => id,
            None => match self.group_shown(self.active_group()?, cx) {
                Shown::Page(id) => id,
                _ => return None,
            },
        };
        store(cx)?
            .get(id)
            .filter(|tab| tab.location.as_ref().is_none_or(Location::is_page))
            .cloned()
    }

    /// Zooms the page the zoom keys reach, as a font size `command` asks.
    /// Returns `false` when there is none, so the command sizes the terminal
    /// font instead. The page keeps the keyboard if it held it.
    pub(crate) fn zoom_focused_page(&mut self, command: Command, cx: &mut Context<Self>) -> bool {
        let change = match command {
            Command::IncreaseFontSize => ZoomChange::In,
            Command::DecreaseFontSize => ZoomChange::Out,
            Command::ResetFontSize => ZoomChange::Reset,
            _ => return false,
        };
        let Some(tab) = self.zoomed_tab(cx) else {
            return false;
        };
        if let Ok(zoom) = tab.zoom.changed(change) {
            Store::update(cx, |store| store.set_zoom(tab.id, zoom));
            self.apply_page_zoom(tab.id, zoom);
        }
        cx.notify();
        true
    }

    /// Draws this window's page for tab `id` at `zoom` now. Pages otherwise
    /// follow their tab's zoom as the window presents them, which it does
    /// not while nothing draws it, as behind a locked screen, and a page
    /// screenshot taken then would miss the zoom.
    pub(crate) fn apply_page_zoom(&self, id: TabId, zoom: Zoom) {
        #[cfg(any(target_os = "macos", windows))]
        self.browser.pages.follow_zoom(&[id], |_| Some(zoom));
        #[cfg(not(any(target_os = "macos", windows)))]
        let _ = (id, zoom);
    }

    /// The toolbar's mark of a zoomed page, which resets it when clicked.
    /// `None` while the page draws at its own size.
    pub(in crate::browser) fn zoom_chip(
        &self,
        slot: super::Slot,
        tab: &Tab,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        if tab.zoom.is_actual() {
            return None;
        }
        let id = tab.id;
        Some(
            div()
                .id("browser-zoom")
                .debug_selector(move || slot.selector("browser-zoom"))
                .flex_none()
                .flex()
                .items_center()
                .h(px(24.))
                .px(px(6.))
                .rounded(px(crate::config::corners::CONTROL))
                .text_color(rgb(self.theme.foreground))
                .cursor_pointer()
                .hover(|s| s.bg(rgb(self.theme.active)))
                .child(format!("{}%", tab.zoom.percent()))
                .on_click(cx.listener(move |this, _, _, cx| {
                    Store::update(cx, |store| store.set_zoom(id, Zoom::default()));
                    this.apply_page_zoom(id, Zoom::default());
                })),
        )
    }
}

#[cfg(test)]
mod tests;
