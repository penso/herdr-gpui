//! Showing native pages to match what the window draws. On macOS a page lies
//! beneath the window's drawing, so every live page shows, and the window's
//! menus, tooltips, toasts, and file-transfer card draw over it. On Windows a
//! page is a child window above the window's drawing, so a page that one of
//! those reaches is hidden while it shows. Only Windows builds carry that.

use crate::HerdrWindow;
#[cfg(windows)]
use crate::browser::TabId;
use gpui::*;

/// Where the window's toasts and file-transfer card drew. They draw after
/// the pages are presented, so each frame presents pages around where they
/// drew the frame before, and one that draws anywhere else asks for another
/// frame. One that no longer shows is left out at once, from the window's
/// state, so its page comes back without waiting a frame.
#[cfg(windows)]
#[derive(Default)]
pub(in crate::browser) struct Overlays {
    /// Drawn since the pages were last presented.
    drawn: Vec<Bounds<Pixels>>,
    /// What the pages were last presented around.
    presented: Vec<Bounds<Pixels>>,
}

/// Whether a page drawn at `page` must be hidden: a menu or tooltip band
/// `cover` covers it, or it reaches under one of `overlays`.
#[cfg(windows)]
pub(in crate::browser) fn covered(
    cover: Option<crate::menu::Cover>,
    overlays: &[Bounds<Pixels>],
    page: Option<Bounds<Pixels>>,
) -> bool {
    cover.is_some_and(|cover| cover.covers(page))
        || page.is_some_and(|page| overlays.iter().any(|overlay| page.intersects(overlay)))
}

/// How far above the status bar its tooltips may reach.
#[cfg(windows)]
const TOOLTIP_BAND: Pixels = px(320.);

impl HerdrWindow {
    /// Shows or hides the native pages to match what the window draws.
    /// Returns whether another frame is needed to settle, while a menu that
    /// hides pages is first laid out.
    pub(crate) fn present_browser(&mut self, cx: &mut Context<Self>) -> bool {
        #[cfg(any(target_os = "macos", windows))]
        {
            self.browser.pages.set_theme(&self.theme);
            let mut live = self.live_pages(cx);
            live.extend(self.shown_chat_pages());
            // A tab's zoom may change in any window, or from the command
            // line, so each window's pages follow the store as they show.
            if let Some(store) = super::store(cx) {
                self.browser
                    .pages
                    .follow_zoom(&live, |id| store.get(id).map(|tab| tab.zoom));
            }
            // On macOS every live page shows, beneath whatever the window
            // draws over it. A menu or dialog covers the whole window and
            // takes every click, so pages refuse drops while one is open.
            #[cfg(target_os = "macos")]
            let (shown, settling) = {
                self.browser.pages.refuse_drops(self.menu.page.is_some());
                (live, false)
            };
            #[cfg(windows)]
            let (shown, settling) = self.uncovered(live);
            self.browser.pages.present(&shown);
            // The pages record where they draw as this frame lays out.
            self.browser.pages.next_frame();
            settling
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            let _ = cx;
            false
        }
    }

    /// The pages of `live` that nothing the window draws reaches, and
    /// whether another frame must settle first: a popover covers what it
    /// covers only once it is laid out.
    #[cfg(windows)]
    fn uncovered(&mut self, live: Vec<TabId>) -> (Vec<TabId>, bool) {
        use crate::menu::Cover;
        let open = self.menu.page;
        let measured_for = std::mem::replace(&mut self.browser.cover_page, open);
        // A dimmed dialog's cover is known before it is laid out, so the
        // pages under it hide in its first frame. A popover's is measured as
        // it is laid out. Without a menu, a status bar tooltip may show
        // above the bar.
        let cover = match (open, self.browser.tooltip_band) {
            (Some(_), _) if self.menu_dims() => Some(Cover::All),
            // The dimming a closed dialog leaves is GPUI's, under the pages:
            // they stay hidden until it has lifted, rather than show over it.
            _ if self.menu_fading(crate::motion::now()) > 0. => Some(Cover::All),
            (Some(_), _) => Some(self.menu.cover.get().settled(measured_for, open)),
            (None, Some(band)) => Some(Cover::Panel(band)),
            (None, None) => None,
        };
        // Toasts and the file-transfer card show over the groups, and would
        // be hidden, and unclickable, under a page they reach.
        let overlays = self.present_overlays();
        let drawn = self.browser.pages.drawn();
        let drawn = drawn.borrow();
        let shown = live
            .into_iter()
            .filter(|id| !covered(cover, &overlays, drawn.get(id).copied()))
            .collect();
        (shown, cover == Some(Cover::Unknown))
    }

    /// Whether something the window draws hides pages: an open menu, a
    /// status bar tooltip, a toast, or the file-transfer card.
    #[cfg(all(test, windows))]
    pub(in crate::browser) fn pages_covered(&self) -> bool {
        self.menu.page.is_some()
            || self.browser.tooltip_band.is_some()
            || !self.browser.overlays.borrow().presented.is_empty()
    }

    /// The overlays pages were last presented around, for tests.
    #[cfg(all(test, windows))]
    pub(crate) fn presented_overlays(&self) -> Vec<Bounds<Pixels>> {
        self.browser.overlays.borrow().presented.clone()
    }

    /// Where the overlays that still show drew, which this frame's pages are
    /// presented around.
    #[cfg(windows)]
    fn present_overlays(&self) -> Vec<Bounds<Pixels>> {
        let mut overlays = self.browser.overlays.borrow_mut();
        let drawn = std::mem::take(&mut overlays.drawn);
        overlays.presented = if self.overlays_shown() {
            drawn
        } else {
            Vec::new()
        };
        overlays.presented.clone()
    }

    /// Records where a stack of toasts or the file-transfer card draws,
    /// filling it, so the pages it reaches hide while it shows. Only Windows
    /// records it: elsewhere no page draws above the window.
    pub(crate) fn overlay_probe(&self) -> Option<impl IntoElement> {
        #[cfg(windows)]
        {
            let overlays = self.browser.overlays.clone();
            Some(
                canvas(
                    move |bounds, window, _| {
                        let mut overlays = overlays.borrow_mut();
                        if !overlays.drawn.contains(&bounds) {
                            overlays.drawn.push(bounds);
                        }
                        // Pages were presented without it there: present
                        // them again now that it is known.
                        if !overlays.presented.contains(&bounds) {
                            window.request_animation_frame();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
        }
        #[cfg(not(windows))]
        None::<Div>
    }

    /// Notes whether the pointer is over the status bar, whose tooltips show
    /// in the band above it. On Windows the pages in the band hide; elsewhere
    /// nothing needs the note.
    pub(crate) fn hover_status_bar(&mut self, hovered: bool, cx: &mut Context<Self>) {
        #[cfg(windows)]
        {
            let band = self
                .browser
                .status_bar
                .get()
                .filter(|_| hovered)
                .map(|bar| {
                    Bounds::new(
                        point(px(0.), bar.top() - TOOLTIP_BAND),
                        size(px(f32::MAX / 4.), TOOLTIP_BAND),
                    )
                });
            if band != self.browser.tooltip_band {
                self.browser.tooltip_band = band;
                cx.notify();
            }
        }
        #[cfg(not(windows))]
        let _ = (hovered, cx);
    }

    /// Records where the status bar draws, filling it. Only Windows records
    /// it, for the band above the bar where its pages hide.
    pub(crate) fn status_bar_probe(&self) -> Option<impl IntoElement> {
        #[cfg(windows)]
        {
            let bar = self.browser.status_bar.clone();
            Some(
                canvas(move |bounds, _, _| bar.set(Some(bounds)), |_, _, _, _| {})
                    .absolute()
                    .inset_0(),
            )
        }
        #[cfg(not(windows))]
        None::<Div>
    }
}
