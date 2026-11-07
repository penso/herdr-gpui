//! The pages a chat shows in its messages: local files, served from their
//! folders as a browser tab's local page is, but owned by the chat rather
//! than the store. They have no tab, so they are never saved, listed, or
//! put in a group; the chat creates one from a click, the window presents
//! it where the chat drew it, and the chat closes it when the user hides it
//! or the chat goes.

use super::{LocalFile, Location, Tab, TabId, Zoom, page::PageElement, store::Place};
use crate::SheprdWindow;
use gpui::{Bounds, Context, IntoElement, Pixels, Styled as _, Window, canvas};

impl SheprdWindow {
    /// Creates page `id` showing `file`; one this window has already does
    /// not change. Runs from input, as creating a page starts the
    /// platform's web content processes.
    pub(crate) fn open_chat_page(
        &mut self,
        id: TabId,
        file: LocalFile,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> crate::Result<()> {
        let tab = Tab {
            id,
            scope: super::scope(&self.endpoints[self.selected_endpoint]),
            workspace_id: "chat".to_owned(),
            location: Some(Location::Local { file }),
            title,
            origin: None,
            place: Place::Group,
            zoom: Zoom::default(),
        };
        self.browser.pages.ensure(&tab, window, cx)
    }

    /// The element page `id` lays out in, once the window has the page; on
    /// Windows it arrives a moment after it is opened.
    pub(crate) fn chat_page_element(&self, id: TabId) -> Option<PageElement> {
        self.browser.pages.element(id)
    }

    pub(crate) fn close_chat_page(&mut self, id: TabId) {
        self.browser.pages.close(id);
    }

    /// Where page `id` drew in the last frame, cropped to what showed.
    pub(crate) fn chat_page_drawn(&self, id: TabId) -> Option<Bounds<Pixels>> {
        self.browser.pages.drawn().borrow().get(&id).copied()
    }

    /// Asks for another frame once this one drew the pages in `ids` other
    /// than the window presented them, as a scroll that brings one into
    /// view or takes it out does: pages are presented from where the last
    /// frame drew them. Paints nothing.
    pub(crate) fn chat_pages_settle(
        &self,
        ids: Vec<TabId>,
        presented: std::rc::Rc<std::cell::RefCell<Vec<TabId>>>,
        height: f32,
    ) -> impl IntoElement {
        let drawn = self.browser.pages.drawn();
        canvas(
            |_, _, _| {},
            move |_, _, window, _| {
                let drawn = drawn.borrow();
                let presented = presented.borrow();
                let moved = ids.iter().any(|id| {
                    let visible = drawn
                        .get(id)
                        .is_some_and(|bounds| chat_page_visible(*bounds, height));
                    visible != presented.contains(id)
                });
                if moved {
                    window.request_animation_frame();
                }
            },
        )
        .absolute()
        .size_0()
    }
}

/// Whether a chat page that drew `drawn`, cropped to what showed, of a page
/// `height` tall, shows: any of it beneath the window's drawing on macOS,
/// which crops it; only the whole of it on Windows, where it lies above.
pub(crate) fn chat_page_visible(drawn: Bounds<Pixels>, height: f32) -> bool {
    let shows = drawn.size.width > gpui::px(0.) && drawn.size.height > gpui::px(0.);
    if cfg!(windows) {
        shows && drawn.size.height >= gpui::px(height - 2.)
    } else {
        shows
    }
}
