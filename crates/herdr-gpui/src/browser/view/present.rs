//! Showing native pages to match what the window draws, and the pictures
//! covered pages leave while a menu is open.

use crate::HerdrWindow;
#[cfg(any(target_os = "macos", windows))]
use crate::browser::TabId;
use gpui::*;

/// A page a menu covers. A native page sits above everything the window
/// draws, so it steps aside for the menu, leaving a picture of itself.
#[cfg(any(target_os = "macos", windows))]
pub(super) enum Freeze {
    /// The picture was asked for, while the page still shows: WebKit only
    /// pictures a page on screen.
    Asked(std::time::Instant),
    /// Only macOS takes pictures.
    #[cfg(target_os = "macos")]
    Ready(std::sync::Arc<RenderImage>),
    /// No picture came in time, or this platform takes none.
    Blank,
}

/// How long a covered page may keep showing while its picture is taken and
/// decoded.
#[cfg(any(target_os = "macos", windows))]
const FREEZE_WAIT: std::time::Duration = std::time::Duration::from_millis(300);

/// Where the window's toasts and file-transfer card drew. They draw after
/// the pages are presented, so each frame presents pages around where they
/// drew the frame before, and one that draws anywhere else asks for another
/// frame. One that no longer shows is left out at once, from the window's
/// state, so its page comes back without waiting a frame.
#[cfg(any(target_os = "macos", windows))]
#[derive(Default)]
pub(in crate::browser) struct Overlays {
    /// Drawn since the pages were last presented.
    drawn: Vec<Bounds<Pixels>>,
    /// What the pages were last presented around.
    presented: Vec<Bounds<Pixels>>,
}

/// Whether a page drawn at `page` must step aside: a menu or tooltip band
/// `cover` covers it, or it reaches under one of `overlays`.
#[cfg(any(target_os = "macos", windows))]
pub(in crate::browser) fn covered(
    cover: Option<crate::menu::Cover>,
    overlays: &[Bounds<Pixels>],
    page: Option<Bounds<Pixels>>,
) -> bool {
    cover.is_some_and(|cover| cover.covers(page))
        || page.is_some_and(|page| overlays.iter().any(|overlay| page.intersects(overlay)))
}

/// How far above the status bar its tooltips may reach.
#[cfg(any(target_os = "macos", windows))]
const TOOLTIP_BAND: Pixels = px(320.);

impl HerdrWindow {
    /// Shows or hides the native pages to match what the window draws. The
    /// pages sit above everything GPUI paints, so a page an open menu covers
    /// steps aside, leaving a picture of itself where it can; a page the menu
    /// does not reach keeps showing. Returns whether another frame is needed
    /// to settle, while a menu is first laid out or a picture is on its way.
    pub(crate) fn present_browser(&mut self, cx: &mut Context<Self>) -> bool {
        #[cfg(any(target_os = "macos", windows))]
        {
            use crate::menu::Cover;
            let live = self.live_pages(cx);
            let open = self.menu.page;
            let measured_for = std::mem::replace(&mut self.browser.cover_page, open);
            // A dimmed dialog's cover is known before it is laid out, so the
            // pages under it step aside in its first frame. A popover's is
            // measured as it is laid out. Without a menu, a status bar
            // tooltip may show above the bar.
            let cover = match (open, self.browser.tooltip_band) {
                (Some(_), _) if self.menu_dims() => Some(Cover::All),
                (Some(_), _) => Some(self.menu.cover.get().settled(measured_for, open)),
                (None, Some(band)) => Some(Cover::Panel(band)),
                (None, None) => None,
            };
            // Toasts and the file-transfer card show over the groups, and
            // would be hidden, and unclickable, under a page they reach.
            let overlays = self.present_overlays();
            if cover.is_none() && overlays.is_empty() {
                self.browser.frozen.clear();
                self.browser.pages.present(&live, cx);
                return false;
            }
            let bounds = self.browser.page_bounds.borrow().clone();
            let now = std::time::Instant::now();
            let mut settling = cover == Some(Cover::Unknown);
            let mut shown = Vec::new();
            for id in live {
                let covered = covered(cover, &overlays, bounds.get(&id).copied());
                if !covered {
                    shown.push(id);
                    continue;
                }
                match self.browser.frozen.get(&id) {
                    None => {
                        let asked = bounds
                            .get(&id)
                            .is_some_and(|page| self.browser.pages.freeze(id, page.size, cx));
                        let freeze = if asked {
                            shown.push(id);
                            settling = true;
                            Freeze::Asked(now)
                        } else {
                            Freeze::Blank
                        };
                        self.browser.frozen.insert(id, freeze);
                    }
                    Some(Freeze::Asked(since)) if now.duration_since(*since) < FREEZE_WAIT => {
                        shown.push(id);
                        settling = true;
                    }
                    Some(Freeze::Asked(_)) => {
                        self.browser.frozen.insert(id, Freeze::Blank);
                    }
                    #[cfg(target_os = "macos")]
                    Some(Freeze::Ready(_)) => {}
                    Some(Freeze::Blank) => {}
                }
            }
            self.browser.pages.present(&shown, cx);
            settling
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        {
            let _ = cx;
            false
        }
    }

    /// Whether something the window draws may cover pages, so those it
    /// covers step aside and show their picture: an open menu, a status
    /// bar tooltip, a toast, or the file-transfer card.
    /// Only macOS freezes covered pages; Windows builds it for the test.
    #[cfg(any(target_os = "macos", all(windows, test)))]
    pub(in crate::browser) fn pages_covered(&self) -> bool {
        self.menu.page.is_some()
            || self.browser.tooltip_band.is_some()
            || !self.browser.overlays.borrow().presented.is_empty()
    }

    /// The overlays pages were last presented around, for tests.
    #[cfg(all(test, any(target_os = "macos", windows)))]
    pub(crate) fn presented_overlays(&self) -> Vec<Bounds<Pixels>> {
        self.browser.overlays.borrow().presented.clone()
    }

    /// Where the overlays that still show drew, which this frame's pages are
    /// presented around.
    #[cfg(any(target_os = "macos", windows))]
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
    /// filling it, so the pages it reaches step aside for it.
    pub(crate) fn overlay_probe(&self) -> impl IntoElement {
        #[cfg(any(target_os = "macos", windows))]
        let overlays = self.browser.overlays.clone();
        canvas(
            move |_bounds, _window, _| {
                #[cfg(any(target_os = "macos", windows))]
                {
                    let mut overlays = overlays.borrow_mut();
                    if !overlays.drawn.contains(&_bounds) {
                        overlays.drawn.push(_bounds);
                    }
                    // Pages were presented without it there: present them
                    // again now that it is known.
                    if !overlays.presented.contains(&_bounds) {
                        _window.request_animation_frame();
                    }
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
    }

    /// Notes whether the pointer is over the status bar, whose tooltips show
    /// in the band above it.
    pub(crate) fn hover_status_bar(&mut self, hovered: bool, cx: &mut Context<Self>) {
        #[cfg(any(target_os = "macos", windows))]
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
        #[cfg(not(any(target_os = "macos", windows)))]
        let _ = (hovered, cx);
    }

    /// Records where the status bar draws, filling it.
    pub(crate) fn status_bar_probe(&self) -> impl IntoElement {
        #[cfg(any(target_os = "macos", windows))]
        let bar = self.browser.status_bar.clone();
        canvas(
            move |_bounds, _, _| {
                #[cfg(any(target_os = "macos", windows))]
                bar.set(Some(_bounds));
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
    }

    /// Decodes a covered page's picture off the UI thread, and has the page
    /// step aside only once it can be painted in the same frame, so there is
    /// never an empty frame between the page and its picture. A picture that
    /// arrives after the menu closed, or after the wait ran out, is dropped.
    #[cfg(target_os = "macos")]
    pub(super) fn page_frozen(&mut self, id: TabId, tiff: Option<Vec<u8>>, cx: &mut Context<Self>) {
        let waiting =
            move |this: &Self| matches!(this.browser.frozen.get(&id), Some(Freeze::Asked(_)));
        if !waiting(self) {
            return;
        }
        let Some(tiff) = tiff else {
            self.browser.frozen.insert(id, Freeze::Blank);
            cx.notify();
            return;
        };
        let frame = cx
            .background_executor()
            .spawn(async move { crate::browser::snapshot::frame(&tiff) });
        cx.spawn(async move |this, cx| {
            let frame = frame.await;
            this.update(cx, |this, cx| {
                if waiting(this) {
                    let freeze = frame.map_or(Freeze::Blank, |frame| {
                        Freeze::Ready(std::sync::Arc::new(frame))
                    });
                    this.browser.frozen.insert(id, freeze);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// The picture a covered page left, while a menu or a tooltip is open.
    #[cfg(any(target_os = "macos", windows))]
    pub(super) fn frozen_picture(&self, id: TabId) -> Option<std::sync::Arc<RenderImage>> {
        match self.browser.frozen.get(&id)? {
            #[cfg(target_os = "macos")]
            Freeze::Ready(image) if self.pages_covered() => Some(image.clone()),
            _ => None,
        }
    }
}
