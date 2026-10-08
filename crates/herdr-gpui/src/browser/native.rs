//! One window's native web views, one per browser tab it has shown. A view
//! is a platform child view layered above the GPUI surface: one per side of a
//! split is visible, and the window hides them while an overlay is open.
use super::{Location, Tab, TabId, WebUrl, location::PREVIEW_SCHEME, preview::Preview};
use gpui::{App, AppContext as _, Entity, Window};
use gpui_wry::WebView;
use std::{
    collections::HashMap,
    rc::Rc,
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
};
use wry::raw_window_handle::HasWindowHandle;

/// Enough for a burst of title and load reports between two window ticks;
/// later ones are dropped rather than queued without bound.
const EVENT_CAPACITY: usize = 64;

/// What a page reported. Handlers run on the platform's callbacks, so they
/// only queue these for the window to apply on its next tick.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Event {
    Title(TabId, String),
    Loaded(TabId, String),
    /// The page asked for a new window, which becomes a new tab.
    NewWindow(TabId, String),
    /// A screenshot for a note, as TIFF bytes, or `None` when WebKit had
    /// none. Only macOS takes them.
    #[cfg(target_os = "macos")]
    Captured(TabId, u64, Option<Vec<u8>>),
    /// A picture of the whole page, as TIFF bytes, shown in its place while
    /// a menu covers it. Only macOS takes them.
    #[cfg(target_os = "macos")]
    Frozen(TabId, Option<Vec<u8>>),
    /// A message the annotation picker posted. Any script in the page can
    /// post one, so it is parsed as untrusted input.
    Posted(TabId, String),
}

/// Longer posts are dropped before they are queued.
const MAX_POST_BYTES: usize = 64 * 1024;

pub(crate) struct Pages {
    pages: HashMap<TabId, Entity<WebView>>,
    shown: Vec<TabId>,
    sender: SyncSender<Event>,
    events: Receiver<Event>,
    /// Reads local pages' files; started with the first page.
    preview: Option<Rc<Preview>>,
}

impl Default for Pages {
    fn default() -> Self {
        let (sender, events) = mpsc::sync_channel(EVENT_CAPACITY);
        Self {
            pages: HashMap::new(),
            shown: Vec::new(),
            sender,
            events,
            preview: None,
        }
    }
}

fn report(sender: &SyncSender<Event>, event: Event) {
    if let Err(TrySendError::Full(event)) = sender.try_send(event) {
        tracing::debug!(?event, "Dropped a browser event");
    }
}

/// Pages leave only for web addresses and the preview scheme. Anything
/// else, such as `file:` or an application's custom scheme, stays unopened
/// rather than reaching the OS.
fn navigable(url: &str) -> bool {
    url == "about:blank"
        || url.starts_with("about:srcdoc")
        || url.starts_with("herdr-preview://localhost/")
        || WebUrl::try_from(url).is_ok()
}

impl Pages {
    pub(crate) fn ids(&self) -> impl Iterator<Item = TabId> + '_ {
        self.pages.keys().copied()
    }

    pub(crate) fn contains(&self, id: TabId) -> bool {
        self.pages.contains_key(&id)
    }

    pub(crate) fn page(&self, id: TabId) -> Option<&Entity<WebView>> {
        self.pages.get(&id)
    }

    /// Creates the page for `tab` if this window has not shown it yet and it
    /// has an address. Creating one starts the platform's web content
    /// processes, so this runs from input and ticks, never from render.
    pub(crate) fn ensure(
        &mut self,
        tab: &Tab,
        window: &mut Window,
        cx: &mut App,
    ) -> crate::Result<()> {
        // A review tab is drawn by the app; only pages get a web view.
        let Some(location) = tab
            .location
            .as_ref()
            .filter(|location| location.is_page())
            .filter(|_| !self.pages.contains_key(&tab.id))
        else {
            return Ok(());
        };
        let id = tab.id;
        let (title, loaded, popup, posted) = (
            self.sender.clone(),
            self.sender.clone(),
            self.sender.clone(),
            self.sender.clone(),
        );
        let preview = match &self.preview {
            Some(preview) => preview.clone(),
            None => {
                let preview = Rc::new(Preview::start()?);
                self.preview = Some(preview.clone());
                preview
            }
        };
        // The folder this page may read. A web page gets none, and a page
        // keeps the folder it was created with wherever it navigates.
        let root = match location {
            Location::Local { file } => Some(file.root().to_owned()),
            Location::Web { .. } | Location::Review { .. } => None,
        };
        let builder = wry::WebViewBuilder::new()
            .with_url(location.page_url())
            .with_asynchronous_custom_protocol(
                PREVIEW_SCHEME.into(),
                move |_, request, responder| {
                    preview.handle(root.as_deref(), &request, responder);
                },
            )
            .with_ipc_handler(move |request| {
                let body = request.into_body();
                if body.len() <= MAX_POST_BYTES {
                    report(&posted, Event::Posted(id, body));
                }
            })
            .with_devtools(cfg!(debug_assertions))
            .with_navigation_handler(|url| navigable(&url))
            .with_new_window_req_handler(move |url, _| {
                report(&popup, Event::NewWindow(id, url));
                wry::NewWindowResponse::Deny
            })
            // Nothing a page offers is saved to disk.
            .with_download_started_handler(|_, _| false)
            .with_document_title_changed_handler(move |text| report(&title, Event::Title(id, text)))
            .with_on_page_load_handler(move |event, url| {
                if matches!(event, wry::PageLoadEvent::Finished) {
                    report(&loaded, Event::Loaded(id, url));
                }
            });
        // Window has an inherent `window_handle` of its own.
        let handle = HasWindowHandle::window_handle(window)?;
        let view = builder.build_as_child(&handle)?;
        let page = cx.new(|cx| {
            let mut page = WebView::new(view, window, cx);
            // A new page appears only when the window presents it.
            page.hide();
            page
        });
        self.pages.insert(id, page);
        Ok(())
    }

    /// Shows the pages in `ids` and hides every other one; an empty list
    /// hides them all.
    pub(crate) fn present(&mut self, ids: &[TabId], cx: &mut App) {
        let ids: Vec<TabId> = ids
            .iter()
            .copied()
            .filter(|id| self.pages.contains_key(id))
            .collect();
        if self.shown == ids {
            return;
        }
        for id in self.shown.iter().filter(|id| !ids.contains(id)) {
            if let Some(page) = self.pages.get(id) {
                page.update(cx, |page, _| page.hide());
            }
        }
        for id in ids.iter().filter(|id| !self.shown.contains(id)) {
            if let Some(page) = self.pages.get(id) {
                page.update(cx, |page, _| page.show());
            }
        }
        self.shown = ids;
    }

    pub(crate) fn close(&mut self, id: TabId) {
        self.shown.retain(|shown| *shown != id);
        // Dropping the entity hides the view; the platform releases it with
        // the last handle.
        self.pages.remove(&id);
    }

    /// Drops the pages of tabs the app no longer has.
    pub(crate) fn retain(&mut self, mut live: impl FnMut(TabId) -> bool) {
        let gone: Vec<_> = self.pages.keys().copied().filter(|id| !live(*id)).collect();
        for id in gone {
            self.close(id);
        }
    }

    pub(crate) fn load(&self, id: TabId, location: &Location, cx: &mut App) {
        let url = location.page_url();
        if let Some(page) = self.pages.get(&id) {
            page.update(cx, |page, _| page.load_url(&url));
        }
    }

    /// Runs one of the app's own scripts in the page. Never page-supplied
    /// text: data goes in as JSON.
    pub(crate) fn script(&self, id: TabId, script: &str, cx: &App) {
        if let Some(page) = self.pages.get(&id)
            && let Err(error) = page.read(cx).raw().evaluate_script(script)
        {
            tracing::debug!(%error, "Browser script failed");
        }
    }

    pub(crate) fn back(&self, id: TabId, cx: &App) {
        self.script(id, "history.back()", cx);
    }

    pub(crate) fn forward(&self, id: TabId, cx: &App) {
        self.script(id, "history.forward()", cx);
    }

    pub(crate) fn reload(&self, id: TabId, cx: &App) {
        if let Some(page) = self.pages.get(&id)
            && let Err(error) = page.read(cx).raw().reload()
        {
            tracing::debug!(%error, "Browser reload failed");
        }
    }

    /// Captures `rect` of the page, reporting it as `Event::Captured` with
    /// `capture` to match it to its note. Returns whether one was asked for:
    /// only macOS can take them.
    pub(crate) fn capture(
        &self,
        id: TabId,
        rect: super::annotate::Rect,
        capture: u64,
        cx: &App,
    ) -> bool {
        #[cfg(target_os = "macos")]
        if let Some(page) = self.pages.get(&id) {
            let sender = self.sender.clone();
            super::snapshot::capture(page.read(cx).raw(), rect, move |tiff| {
                report(&sender, Event::Captured(id, capture, tiff));
            });
            return true;
        }
        let _ = (id, rect, capture, cx);
        false
    }

    /// Asks for a picture of the whole page, `size` in CSS pixels, reported
    /// as `Event::Frozen`. Returns whether one was asked for: only macOS
    /// takes them.
    pub(crate) fn freeze(&self, id: TabId, size: gpui::Size<gpui::Pixels>, cx: &App) -> bool {
        #[cfg(target_os = "macos")]
        if let Some(page) = self.pages.get(&id) {
            let sender = self.sender.clone();
            let rect = super::annotate::Rect {
                x: 0.,
                y: 0.,
                width: f64::from(f32::from(size.width)),
                height: f64::from(f32::from(size.height)),
            };
            super::snapshot::capture(page.read(cx).raw(), rect, move |tiff| {
                report(&sender, Event::Frozen(id, tiff));
            });
            return true;
        }
        let _ = (id, size, cx);
        false
    }

    /// Hands the keyboard back from the page to the window.
    pub(crate) fn blur(&self, id: TabId, cx: &App) {
        if let Some(page) = self.pages.get(&id) {
            let _ = page.read(cx).raw().focus_parent();
        }
    }

    /// Moves keyboard input into the shown page.
    pub(crate) fn focus(&self, id: TabId, cx: &App) {
        if let Some(page) = self.pages.get(&id) {
            let _ = page.read(cx).raw().focus();
        }
    }

    /// Everything the pages reported since the last call.
    pub(crate) fn drain(&self) -> impl Iterator<Item = Event> + '_ {
        self.events.try_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_only_navigate_to_web_addresses() {
        for allowed in [
            "https://a.test/",
            "http://localhost:3000/x",
            "about:blank",
            "herdr-preview://localhost/index.html",
        ] {
            assert!(navigable(allowed), "{allowed}");
        }
        for denied in [
            "file:///etc/passwd",
            "vscode://file/x",
            "mailto:a@b.test",
            "data:text/html,x",
            "herdr-preview://elsewhere/x",
        ] {
            assert!(!navigable(denied), "{denied}");
        }
    }

    #[test]
    fn page_reports_are_bounded() {
        let pages = Pages::default();
        for index in 0..EVENT_CAPACITY + 10 {
            report(
                &pages.sender,
                Event::Title(TabId::test(0), index.to_string()),
            );
        }
        assert_eq!(pages.drain().count(), EVENT_CAPACITY);
        assert_eq!(pages.drain().count(), 0);
    }
}
