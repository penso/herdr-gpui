//! One window's native web views, one per browser tab it has shown. One per
//! side of a split is visible. On macOS each lies beneath the window's own
//! drawing; on Windows each lies above it, and the window hides the ones its
//! overlays reach. See [`super::page`].
use super::{
    Location, Tab, TabId, WebUrl, Zoom,
    location::PREVIEW_SCHEME,
    page::{Drawn, Page, PageElement},
    preview::Preview,
};
use crate::HerdrWindow;
use gpui::{Context, Window};
use std::{
    collections::HashMap,
    rc::Rc,
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
};
#[cfg(target_os = "macos")]
use std::{
    collections::HashSet,
    sync::{Arc, Mutex, PoisonError},
};
use wry::raw_window_handle::HasWindowHandle;

/// Enough for a burst of title and load reports between two window ticks;
/// later ones are dropped rather than queued without bound.
const EVENT_CAPACITY: usize = 64;

/// What a page reported, from the tab `P` names. Handlers run on the
/// platform's callbacks, so they only queue these for the window to apply on
/// its next tick.
#[derive(Debug)]
pub(crate) enum Event<P = TabId> {
    Title(P, String),
    Loaded(P, String),
    /// The page asked for a new window, which becomes a new, unrelated tab.
    /// macOS asks for this only when it cannot build the popup itself.
    NewWindow(P, String),
    /// The page opened a popup, already built and linked to it, which the
    /// window adopts as a new tab. Only macOS builds them.
    #[cfg(target_os = "macos")]
    Opened(P, WebUrl, Source, super::popup::Popup),
    /// A popup's script closed it, which closes its tab.
    #[cfg(target_os = "macos")]
    Closed(P),
    /// A screenshot for a note, as TIFF bytes, or `None` when WebKit had
    /// none. Only macOS takes them.
    #[cfg(target_os = "macos")]
    Captured(P, u64, Option<Vec<u8>>),
    /// A message the annotation picker or the page bridge posted, and the
    /// frame that posted it. Any script in any frame can post one, so it is
    /// parsed as untrusted input.
    Posted(P, String, super::PostedFrom),
}

/// The page a report came from. A popup's page exists before its tab does,
/// so its reports name the popup until the window adopts it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Source {
    Tab(TabId),
    #[cfg(target_os = "macos")]
    Popup(u64),
}

/// Popups are numbered across windows, so a key never names two.
#[cfg(target_os = "macos")]
static NEXT_POPUP: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl Event<Source> {
    /// The same report from the tab `tab` finds for its source, or `None`
    /// for a popup no tab adopted.
    fn resolve(self, tab: impl Fn(Source) -> Option<TabId>) -> Option<Event> {
        Some(match self {
            Self::Title(source, title) => Event::Title(tab(source)?, title),
            Self::Loaded(source, url) => Event::Loaded(tab(source)?, url),
            Self::NewWindow(source, url) => Event::NewWindow(tab(source)?, url),
            #[cfg(target_os = "macos")]
            Self::Opened(source, url, popup, page) => Event::Opened(tab(source)?, url, popup, page),
            #[cfg(target_os = "macos")]
            Self::Closed(source) => Event::Closed(tab(source)?),
            #[cfg(target_os = "macos")]
            Self::Captured(source, capture, tiff) => Event::Captured(tab(source)?, capture, tiff),
            Self::Posted(source, body, from) => Event::Posted(tab(source)?, body, from),
        })
    }
}

/// Longer posts are dropped before they are queued.
const MAX_POST_BYTES: usize = 64 * 1024;

/// A page being built.
// Only Windows builds pages afterwards; elsewhere none is ever built.
#[cfg_attr(not(windows), allow(dead_code))]
pub(super) struct Build {
    number: u64,
    /// Where the page was asked to go while it was built.
    pub(super) location: Option<Location>,
}

pub(crate) struct Pages {
    pages: HashMap<TabId, Rc<Page>>,
    /// The pages being built, outside the App's borrow. Only Windows builds
    /// pages that way; see `super::building`.
    building: HashMap<TabId, Build>,
    /// Numbers each build, so a build's page is kept only while it is still
    /// its tab's latest build.
    #[cfg_attr(not(windows), allow(dead_code))]
    builds: u64,
    shown: Vec<TabId>,
    /// Where each page drew in the frame being drawn.
    drawn: Drawn,
    outbox: Outbox,
    events: Receiver<Event<Source>>,
    /// The tab each adopted popup became.
    #[cfg(target_os = "macos")]
    adopted: HashMap<u64, TabId>,
    /// Each adopted popup's UI delegate, which its view holds only weakly.
    #[cfg(target_os = "macos")]
    delegates: HashMap<TabId, objc2::rc::Retained<super::popup::Closer>>,
    /// Where each hidden page was before it was moved out of the window.
    #[cfg(target_os = "macos")]
    parked: HashMap<TabId, super::parking::Parked>,
    /// Reads local pages' files; started with the first page.
    preview: Option<Rc<Preview>>,
    /// The theme the kit serves local pages, and the colors it came from.
    theme: (super::kit::ThemeKey, String),
}

impl Default for Pages {
    fn default() -> Self {
        let (sender, events) = mpsc::sync_channel(EVENT_CAPACITY);
        Self {
            pages: HashMap::new(),
            building: HashMap::new(),
            builds: 0,
            shown: Vec::new(),
            drawn: Drawn::default(),
            outbox: Outbox {
                sender,
                #[cfg(target_os = "macos")]
                closed: Arc::default(),
            },
            events,
            #[cfg(target_os = "macos")]
            adopted: HashMap::new(),
            #[cfg(target_os = "macos")]
            delegates: HashMap::new(),
            #[cfg(target_os = "macos")]
            parked: HashMap::new(),
            preview: None,
            theme: Default::default(),
        }
    }
}

/// Where a page's handlers send what it reports.
#[derive(Clone)]
struct Outbox {
    sender: SyncSender<Event<Source>>,
    /// Popups whose script closed them. A close must not be dropped with
    /// the queue full, or its tab would stay, so these wait here instead:
    /// at most one entry per popup.
    #[cfg(target_os = "macos")]
    closed: Arc<Mutex<HashSet<u64>>>,
}

fn report(sender: &SyncSender<Event<Source>>, event: Event<Source>) {
    if let Err(TrySendError::Full(event)) = sender.try_send(event) {
        tracing::debug!(?event, "Dropped a browser event");
    }
}

/// The handlers every page has, its own or a popup's, reporting as `source`.
fn with_handlers<'a>(
    builder: wry::WebViewBuilder<'a>,
    source: Source,
    outbox: &Outbox,
) -> wry::WebViewBuilder<'a> {
    let (title, loaded, popup) = (outbox.sender.clone(), outbox.sender.clone(), outbox.clone());
    builder
        .with_devtools(cfg!(debug_assertions))
        .with_navigation_handler(|url| navigable(&url))
        .with_new_window_req_handler(move |url, features| new_window(source, url, features, &popup))
        // Nothing a page offers is saved to disk.
        .with_download_started_handler(|_, _| false)
        .with_document_title_changed_handler(move |text| {
            report(&title, Event::Title(source, text));
        })
        .with_on_page_load_handler(move |event, url| {
            if matches!(event, wry::PageLoadEvent::Finished) {
                report(&loaded, Event::Loaded(source, url));
            }
        })
}

/// Answers a page's request for a new window. On macOS the popup is built
/// now, linked to its opener; elsewhere, or when that fails, the address
/// opens in a new, unrelated tab.
fn new_window(
    opener: Source,
    url: String,
    features: wry::NewWindowFeatures,
    outbox: &Outbox,
) -> wry::NewWindowResponse {
    #[cfg(target_os = "macos")]
    if let (Some(mtm), Ok(address)) = (objc2::MainThreadMarker::new(), WebUrl::try_from(&*url)) {
        let key = NEXT_POPUP.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let source = Source::Popup(key);
        let builder = with_handlers(wry::WebViewBuilder::new(), source, outbox);
        let closed = outbox.closed.clone();
        let built = super::popup::build(
            builder,
            features.opener,
            move || {
                closed
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .insert(key);
            },
            mtm,
        );
        match built {
            Ok(popup) => {
                let webview = popup.webview(mtm);
                // A popup the window never hears of would stay hidden.
                return match outbox
                    .sender
                    .try_send(Event::Opened(opener, address, source, popup))
                {
                    Ok(()) => wry::NewWindowResponse::Create { webview },
                    Err(_) => wry::NewWindowResponse::Deny,
                };
            }
            Err(error) => tracing::debug!(%error, "Cannot build a popup"),
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = features;
    report(&outbox.sender, Event::NewWindow(opener, url));
    wry::NewWindowResponse::Deny
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
    /// The tabs this window has a page for, or is building one for.
    pub(crate) fn ids(&self) -> impl Iterator<Item = TabId> + '_ {
        self.pages.keys().chain(self.building.keys()).copied()
    }

    pub(crate) fn contains(&self, id: TabId) -> bool {
        self.pages.contains_key(&id)
    }

    /// Tab `id`'s page, for the control requests.
    pub(crate) fn page(&self, id: TabId) -> Option<&Rc<Page>> {
        self.pages.get(&id)
    }

    /// The element tab `id`'s page lays out in, if the window has the page.
    pub(crate) fn element(&self, id: TabId) -> Option<PageElement> {
        let page = self.pages.get(&id)?;
        Some(PageElement::new(id, page.clone(), self.drawn.clone()))
    }

    /// Where each page drew in the frame being drawn, filled as they lay out.
    pub(crate) fn drawn(&self) -> Drawn {
        self.drawn.clone()
    }

    /// Where tab `id`'s page drew last, if it is laid out.
    #[cfg(target_os = "macos")]
    pub(crate) fn drawn_at(&self, id: TabId) -> Option<gpui::Bounds<gpui::Pixels>> {
        self.drawn.borrow().get(&id).copied()
    }

    /// Starts the next frame's record of where pages draw. The window calls
    /// it as it presents the pages, before they lay out.
    pub(crate) fn next_frame(&self) {
        self.drawn.borrow_mut().clear();
    }

    /// Whether tab `id`'s page is being built.
    pub(crate) fn is_building(&self, id: TabId) -> bool {
        self.building.contains_key(&id)
    }

    /// Whether tab `id`'s page is on screen, not hidden.
    #[cfg(target_os = "macos")]
    pub(crate) fn is_shown(&self, id: TabId) -> bool {
        self.shown.contains(&id)
    }

    /// Creates the page for `tab` if this window has not shown it yet and it
    /// has an address. Creating one starts the platform's web content
    /// processes, so this runs from input and ticks, never from render. On
    /// Windows the page is built afterwards, outside the App's borrow, and
    /// arrives when built; see `super::building`.
    pub(crate) fn ensure(
        &mut self,
        tab: &Tab,
        window: &mut Window,
        cx: &mut Context<HerdrWindow>,
    ) -> crate::Result<()> {
        // A review tab is drawn by the app; only pages get a web view.
        let Some(location) = tab
            .location
            .as_ref()
            .filter(|location| location.is_page())
            .filter(|_| !self.pages.contains_key(&tab.id) && !self.is_building(tab.id))
        else {
            return Ok(());
        };
        let id = tab.id;
        let posted = self.outbox.sender.clone();
        let preview = match &self.preview {
            Some(preview) => preview.clone(),
            None => {
                let preview = Rc::new(Preview::start()?);
                preview.set_theme(self.theme.1.clone());
                self.preview = Some(preview.clone());
                preview
            }
        };
        // The folder this page may read. A web page gets none, and a page
        // keeps the folder it was created with wherever it navigates.
        let root = match location {
            Location::Local { file } => Some(file.root().to_owned()),
            Location::Web { .. }
            | Location::Review { .. }
            | Location::Code { .. }
            | Location::Orchestrator { .. }
            | Location::Devices
            | Location::CoreTerminal { .. } => None,
        };
        // Only a local page, which the agent wrote, gets `herdr.send`.
        let bridge = if root.is_some() {
            super::annotate::BRIDGE
        } else {
            ""
        };
        let builder = with_handlers(wry::WebViewBuilder::new(), Source::Tab(id), &self.outbox)
            .with_initialization_script(bridge)
            .with_url(location.page_url())
            .with_asynchronous_custom_protocol(
                PREVIEW_SCHEME.into(),
                move |_, request, responder| {
                    preview.handle(root.as_deref(), &request, responder);
                },
            )
            .with_ipc_handler(move |request| {
                // wry names the frame that posted, not the page's main frame.
                let from = super::PostedFrom::of(&request.uri().to_string());
                let body = request.into_body();
                if body.len() <= MAX_POST_BYTES {
                    report(&posted, Event::Posted(Source::Tab(id), body, from));
                }
            });
        // Window has an inherent `window_handle` of its own.
        let handle = HasWindowHandle::window_handle(window)?;
        #[cfg(target_os = "macos")]
        {
            let _ = cx;
            let view = builder.build_as_child(&handle)?;
            super::page_keys::install();
            self.add(id, view, window)
        }
        #[cfg(windows)]
        {
            let _ = handle;
            self.builds += 1;
            let build = Build {
                number: self.builds,
                location: None,
            };
            self.building.insert(id, build);
            super::building::start(id, self.builds, builder, window, cx);
            Ok(())
        }
    }

    /// Makes `view` tab `id`'s page, hidden until the window presents it.
    pub(super) fn add(
        &mut self,
        id: TabId,
        view: wry::WebView,
        window: &Window,
    ) -> crate::Result<()> {
        self.pages.insert(id, Rc::new(Page::new(view, window)?));
        self.hide(id);
        Ok(())
    }

    /// Ends build `number` of tab `id`, returning it if its page is still
    /// wanted: a tab closed while its page was built wants none, and one
    /// built again since wants only the later page.
    #[cfg(windows)]
    pub(super) fn finish_building(&mut self, id: TabId, number: u64) -> Option<Build> {
        if self.building.get(&id)?.number != number {
            return None;
        }
        self.building.remove(&id)
    }

    /// Serves `theme` to local pages from now on: a page picks it up when it
    /// next loads. Cheap when the colors did not change.
    pub(crate) fn set_theme(&mut self, theme: &crate::config::Theme) {
        let key = super::kit::theme_key(theme);
        if self.theme.0 == key && !self.theme.1.is_empty() {
            return;
        }
        self.theme = (key, super::kit::theme_css(theme));
        if let Some(preview) = &self.preview {
            preview.set_theme(self.theme.1.clone());
        }
    }

    /// Hides tab `id`'s page. On macOS it also leaves the window's area,
    /// where a hidden view still takes drags; see [`super::parking`].
    fn hide(&mut self, id: TabId) {
        let Some(page) = self.pages.get(&id) else {
            return;
        };
        page.hide();
        // A page already hidden keeps the place it left the first time.
        #[cfg(target_os = "macos")]
        self.parked
            .entry(id)
            .or_insert_with(|| super::parking::park(page.raw()));
    }

    /// Shows tab `id`'s page, back where it was.
    fn show(&mut self, id: TabId) {
        let Some(page) = self.pages.get(&id) else {
            return;
        };
        #[cfg(target_os = "macos")]
        if let Some(place) = self.parked.remove(&id) {
            super::parking::unpark(page.raw(), place);
        }
        page.show();
    }

    /// Refuses drops on every page, as while a menu or dialog covers the
    /// window, or takes them again.
    #[cfg(target_os = "macos")]
    pub(crate) fn refuse_drops(&self, refuse: bool) {
        for page in self.pages.values() {
            page.refuse_drops(refuse);
        }
    }

    /// Shows the pages in `ids` and hides every other one; an empty list
    /// hides them all.
    pub(crate) fn present(&mut self, ids: &[TabId]) {
        let ids: Vec<TabId> = ids
            .iter()
            .copied()
            .filter(|id| self.pages.contains_key(id))
            .collect();
        if self.shown == ids {
            return;
        }
        let shown = std::mem::take(&mut self.shown);
        for id in shown.iter().filter(|id| !ids.contains(id)) {
            self.hide(*id);
        }
        for id in ids.iter().filter(|id| !shown.contains(id)) {
            self.show(*id);
        }
        self.shown = ids;
    }

    /// Makes `popup` the page of tab `id`, the tab the window opened for it.
    #[cfg(target_os = "macos")]
    pub(crate) fn adopt(
        &mut self,
        id: TabId,
        source: Source,
        popup: super::popup::Popup,
        window: &mut Window,
    ) -> crate::Result<()> {
        let mtm = objc2::MainThreadMarker::new().ok_or(crate::Error::PageStack)?;
        let (view, delegate, place) = popup.into_parts(mtm);
        // It was built hidden, and appears when the window presents it.
        self.pages.insert(id, Rc::new(Page::new(view, window)?));
        self.delegates.insert(id, delegate);
        self.parked.insert(id, place);
        if let Source::Popup(key) = source {
            self.adopted.insert(key, id);
        }
        Ok(())
    }

    pub(crate) fn close(&mut self, id: TabId) {
        #[cfg(target_os = "macos")]
        {
            self.adopted.retain(|_, tab| *tab != id);
            self.delegates.remove(&id);
            self.parked.remove(&id);
        }
        self.building.remove(&id);
        self.shown.retain(|shown| *shown != id);
        self.drawn.borrow_mut().remove(&id);
        // Dropping the page hides it; the platform releases its view with
        // the last handle.
        self.pages.remove(&id);
    }

    /// Drops the pages of tabs the app no longer has.
    pub(crate) fn retain(&mut self, mut live: impl FnMut(TabId) -> bool) {
        let gone: Vec<_> = self.ids().filter(|id| !live(*id)).collect();
        for id in gone {
            self.close(id);
        }
    }

    /// Loads `location` in tab `id`'s page. A page still being built loads
    /// the latest such location once built.
    pub(crate) fn load(&mut self, id: TabId, location: &Location) {
        if let Some(build) = self.building.get_mut(&id) {
            build.location = Some(location.clone());
            return;
        }
        let url = location.page_url();
        if let Some(page) = self.pages.get(&id)
            && let Err(error) = page.raw().load_url(&url)
        {
            tracing::debug!(%error, "Browser navigation failed");
        }
    }

    /// Runs one of the app's own scripts in the page. Never page-supplied
    /// text: data goes in as JSON.
    pub(crate) fn script(&self, id: TabId, script: &str) {
        if let Some(page) = self.pages.get(&id)
            && let Err(error) = page.raw().evaluate_script(script)
        {
            tracing::debug!(%error, "Browser script failed");
        }
    }

    pub(crate) fn back(&self, id: TabId) {
        self.script(id, "history.back()");
    }

    pub(crate) fn forward(&self, id: TabId) {
        self.script(id, "history.forward()");
    }

    pub(crate) fn reload(&self, id: TabId) {
        if let Some(page) = self.pages.get(&id)
            && let Err(error) = page.raw().reload()
        {
            tracing::debug!(%error, "Browser reload failed");
        }
    }

    /// Captures `rect` of the page, reporting it as `Event::Captured` with
    /// `capture` to match it to its note. Returns whether one was asked for:
    /// only macOS can take them.
    pub(crate) fn capture(&self, id: TabId, rect: super::annotate::Rect, capture: u64) -> bool {
        #[cfg(target_os = "macos")]
        if let Some(page) = self.pages.get(&id) {
            let sender = self.outbox.sender.clone();
            // The picker measures `rect` in CSS pixels, which a zoomed page
            // draws larger in the view.
            let rect = page.drawn_zoom().view_rect(rect);
            super::snapshot::capture(page.raw(), rect, move |tiff| {
                report(&sender, Event::Captured(Source::Tab(id), capture, tiff));
            });
            return true;
        }
        let _ = (id, rect, capture);
        false
    }

    /// The tab whose shown page holds the window's keyboard, if one does.
    pub(crate) fn keyboard_page(&self) -> Option<TabId> {
        self.shown
            .iter()
            .copied()
            .find(|id| self.pages.get(id).is_some_and(|page| page.holds_keyboard()))
    }

    /// Draws each page of `ids` at the zoom `zoom` gives its tab.
    pub(crate) fn follow_zoom(&self, ids: &[TabId], zoom: impl Fn(TabId) -> Option<Zoom>) {
        for id in ids {
            if let (Some(page), Some(zoom)) = (self.pages.get(id), zoom(*id)) {
                page.zoom(zoom);
            }
        }
    }

    /// Hands the keyboard back from the page to the window.
    pub(crate) fn blur(&self, id: TabId) {
        if let Some(page) = self.pages.get(&id) {
            page.blur();
        }
    }

    /// Moves keyboard input into the shown page.
    pub(crate) fn focus(&self, id: TabId) {
        if let Some(page) = self.pages.get(&id) {
            page.focus();
        }
    }

    /// The next thing the pages reported. One at a time, as the window
    /// adopts a popup between its report and the popup's own.
    pub(crate) fn next_event(&self) -> Option<Event> {
        while let Ok(event) = self.events.try_recv() {
            let resolved = event.resolve(|source| match source {
                Source::Tab(id) => Some(id),
                #[cfg(target_os = "macos")]
                Source::Popup(key) => self.adopted.get(&key).copied(),
            });
            if resolved.is_some() {
                return resolved;
            }
        }
        #[cfg(target_os = "macos")]
        return self.next_close();
        #[cfg(not(target_os = "macos"))]
        None
    }

    /// A popup that closed itself, once the queue is empty: its `Opened`
    /// report, queued before the popup ran any script, has been applied by
    /// then, so a popup no tab adopted is forgotten.
    #[cfg(target_os = "macos")]
    fn next_close(&self) -> Option<Event> {
        let mut closed = self
            .outbox
            .closed
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        while let Some(key) = closed.iter().next().copied() {
            closed.remove(&key);
            if let Some(tab) = self.adopted.get(&key) {
                return Some(Event::Closed(*tab));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
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
                &pages.outbox.sender,
                Event::Title(Source::Tab(TabId::test(0)), index.to_string()),
            );
        }
        assert_eq!(
            std::iter::from_fn(|| pages.next_event()).count(),
            EVENT_CAPACITY
        );
        assert!(pages.next_event().is_none());
    }

    #[cfg(windows)]
    #[test]
    fn a_page_closed_while_built_is_not_kept() {
        let mut pages = Pages::default();
        let (kept, closed, gone) = (TabId::test(1), TabId::test(2), TabId::test(3));
        for (number, id) in (1..).zip([kept, closed, gone]) {
            pages.building.insert(id, build(number));
        }
        assert!(pages.is_building(kept));
        // A tab being built is one the window has, so closing or dropping
        // it reaches the build.
        assert_eq!(pages.ids().count(), 3);
        pages.close(closed);
        pages.retain(|id| id != gone);
        assert!(!pages.is_building(closed) && !pages.is_building(gone));
        assert!(pages.finish_building(kept, 1).is_some());
        assert!(pages.finish_building(closed, 2).is_none());
        assert!(pages.finish_building(gone, 3).is_none());
        // A build finishes once.
        assert!(pages.finish_building(kept, 1).is_none());
        assert!(!pages.is_building(kept));
    }

    #[cfg(windows)]
    #[test]
    fn only_a_tabs_latest_build_is_kept() {
        let mut pages = Pages::default();
        let tab = TabId::test(1);
        // Closed while built, as a changed VS Code server closes its page,
        // then built again: the first build finishing first leaves the
        // second one building.
        pages.building.insert(tab, build(1));
        pages.close(tab);
        pages.building.insert(tab, build(2));
        assert!(pages.finish_building(tab, 1).is_none());
        assert!(pages.is_building(tab));
        assert!(pages.finish_building(tab, 2).is_some());
    }

    #[cfg(windows)]
    #[test]
    fn a_page_built_goes_where_it_was_last_sent() {
        let mut pages = Pages::default();
        let tab = TabId::test(1);
        pages.building.insert(tab, build(1));
        let web = |url: &str| Location::Web {
            url: WebUrl::try_from(url).unwrap(),
        };
        pages.load(tab, &web("https://a.test/"));
        pages.load(tab, &web("https://b.test/"));
        let built = pages.finish_building(tab, 1).map(|build| build.location);
        assert_eq!(built, Some(Some(web("https://b.test/"))));
    }

    #[cfg(windows)]
    fn build(number: u64) -> Build {
        Build {
            number,
            location: None,
        }
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn popup_reports_wait_for_their_tab() {
        let mut pages = Pages::default();
        let (popup, tab) = (Source::Popup(7), TabId::test(3));
        let title = |text: &str| Event::Title(popup, text.to_owned());
        // A popup no tab adopted reports to no one.
        report(&pages.outbox.sender, title("before"));
        assert!(pages.next_event().is_none());
        pages.adopted.insert(7, tab);
        report(&pages.outbox.sender, title("after"));
        pages
            .outbox
            .closed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(7);
        assert!(
            matches!(pages.next_event(), Some(Event::Title(id, text)) if id == tab && text == "after")
        );
        assert!(matches!(pages.next_event(), Some(Event::Closed(id)) if id == tab));
        // Its tab closing forgets it.
        pages.close(tab);
        report(&pages.outbox.sender, title("gone"));
        assert!(pages.next_event().is_none());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn popup_closes_survive_a_full_queue() {
        let mut pages = Pages::default();
        let tab = TabId::test(4);
        pages.adopted.insert(5, tab);
        for index in 0..EVENT_CAPACITY + 10 {
            report(
                &pages.outbox.sender,
                Event::Title(Source::Popup(5), index.to_string()),
            );
        }
        // A close no tab adopted is forgotten rather than kept.
        pages
            .outbox
            .closed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend([5, 6]);
        let events: Vec<Event> = std::iter::from_fn(|| pages.next_event()).collect();
        assert_eq!(events.len(), EVENT_CAPACITY + 1);
        assert!(matches!(events.last(), Some(Event::Closed(id)) if *id == tab));
        assert!(
            pages
                .outbox
                .closed
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_empty()
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn tab_reports_skip_unadopted_popups_in_order() {
        let pages = Pages::default();
        let tab = TabId::test(1);
        report(
            &pages.outbox.sender,
            Event::Title(Source::Popup(9), "lost".into()),
        );
        report(
            &pages.outbox.sender,
            Event::Loaded(Source::Tab(tab), "https://a.test/".into()),
        );
        assert!(matches!(pages.next_event(), Some(Event::Loaded(id, _)) if id == tab));
        assert!(pages.next_event().is_none());
    }
}
