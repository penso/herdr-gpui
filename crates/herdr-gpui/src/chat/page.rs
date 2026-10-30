//! Web pages shown inside a chat: an HTML fence the agent wrote, or an
//! HTML file one of its tool calls wrote. Where pages show in the window
//! (macOS and Windows), one opens live in the message, in place of its card,
//! and scrolls with it; elsewhere it opens in the system browser. Either is
//! copied first to a file of its own, in a folder only this user can read,
//! so the page's script can read nothing beside it: an HTML file's siblings
//! stay out of reach, at the cost of the stylesheets and scripts it loads
//! from them.
//!
//! A page in a message lies beneath the window's drawing on macOS, so the
//! list's clip crops it like any row; on Windows it lies above, so it shows
//! only while the whole of it is in view. Either way it shows only where
//! the last frame drew it, and a frame that moves it asks for one more.
//!
//! The page is the agent's, so it is untrusted: it only ever loads because
//! the user asked, and it gets what a local browser tab gets, no more.

use crate::{HerdrWindow, browser::TabId, notifications::safe_text};
use gpui::{AppContext as _, Context, Window};
use herdr_chat_protocol::EntryId;
use std::{
    hash::{DefaultHasher, Hash, Hasher},
    path::{Path, PathBuf},
    rc::Rc,
};

/// How tall a page draws in a message.
pub(crate) const PAGE_HEIGHT: f32 = 420.;
/// Pages a chat keeps live; showing another closes the oldest.
#[cfg(any(target_os = "macos", windows))]
const MAX_LIVE: usize = 4;

/// Where in the transcript a page shows: the part of an agent message that
/// holds the fence, or [`PageKey::TOOL`] under a tool call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PageKey {
    pub(crate) entry: EntryId,
    pub(crate) part: usize,
}

impl PageKey {
    /// The part a tool call's page takes.
    pub(crate) const TOOL: usize = usize::MAX;
}

/// A page live in a chat's message. Linux shows pages in the system
/// browser, so a chat there never has one.
#[derive(Debug)]
#[cfg_attr(not(any(target_os = "macos", windows, test)), allow(dead_code))]
pub(crate) struct ChatPage {
    pub(crate) key: PageKey,
    pub(crate) id: TabId,
}

impl ChatPage {
    /// A number for the element ids of the page's controls.
    pub(crate) fn id_number(&self) -> u64 {
        self.id.number()
    }
}

/// What to show.
#[derive(Clone, Debug)]
pub(crate) enum PageSource {
    /// An HTML fence's source.
    Html(Rc<str>),
    /// An HTML file on the chat's machine.
    File(PathBuf),
}

impl HerdrWindow {
    /// Shows `source` in the message at `key` of the chat in `pane_id`, or
    /// in the system browser where pages cannot show in the window.
    pub(crate) fn show_chat_page(
        &mut self,
        pane_id: &str,
        key: PageKey,
        source: PageSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        let copied = match source {
            PageSource::File(path) => {
                if let super::Route::Ssh(host) = &pane.target.route {
                    pane.error = Some(format!(
                        "That page is on {}; open it there to see it",
                        safe_text(host, 80)
                    ));
                    cx.notify();
                    return;
                }
                cx.background_spawn(async move {
                    let html = read_page(&path)?;
                    write_page(&html)
                })
            }
            PageSource::Html(html) => {
                let html = html.as_bytes().to_vec();
                cx.background_spawn(async move { write_page(&html) })
            }
        };
        let pane_id = pane_id.to_owned();
        cx.spawn_in(window, async move |this, cx| {
            let copied = copied.await;
            let _ = this.update_in(cx, |this, window, cx| match copied {
                Ok(path) => this.open_chat_file(&pane_id, key, path, window, cx),
                Err(error) => this.chat_page_failed(&pane_id, &error, cx),
            });
        })
        .detach();
    }

    fn chat_page_failed(
        &mut self,
        pane_id: &str,
        error: &dyn std::fmt::Display,
        cx: &mut Context<Self>,
    ) {
        if let Some(pane) = self.chats.pane_mut(pane_id) {
            pane.error = Some(format!("The page could not be shown: {error}"));
        }
        cx.notify();
    }

    #[cfg(any(target_os = "macos", windows))]
    fn open_chat_file(
        &mut self,
        pane_id: &str,
        key: PageKey,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let home = crate::config::home().ok();
        let file = match crate::browser::LocalFile::new(&path, home.as_deref()) {
            Ok(file) => file,
            Err(error) => return self.chat_page_failed(pane_id, &error, cx),
        };
        let open = self
            .chats
            .pane(pane_id)
            .is_some_and(|pane| pane.pages.iter().any(|page| page.key == key));
        if open {
            return;
        }
        self.chats.opened_pages += 1;
        let id = TabId::panel(self.chats.opened_pages);
        if let Err(error) = self.open_chat_page(id, file, "HTML page".to_owned(), window, cx) {
            return self.chat_page_failed(pane_id, &error, cx);
        }
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            self.close_chat_page(id);
            return;
        };
        pane.pages.push(ChatPage { key, id });
        pane.remeasure(key.entry);
        let oldest = (pane.pages.len() > MAX_LIVE).then(|| pane.pages.remove(0));
        if let Some(oldest) = oldest {
            pane.remeasure(oldest.key.entry);
            self.close_chat_page(oldest.id);
        }
        cx.notify();
    }

    /// Without pages in the window, the system browser shows the file.
    #[cfg(not(any(target_os = "macos", windows)))]
    fn open_chat_file(
        &mut self,
        pane_id: &str,
        _key: PageKey,
        path: PathBuf,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match url::Url::from_file_path(&path) {
            Ok(url) => {
                cx.open_url(url.as_str());
                self.show_flash(
                    crate::window::Flash::success("Opened the page in your browser"),
                    cx,
                );
            }
            Err(()) => self.chat_page_failed(pane_id, &"not a file path", cx),
        }
    }

    /// Closes the page at `key` of the chat in `pane_id`, back to its card.
    pub(crate) fn hide_chat_page(&mut self, pane_id: &str, key: PageKey, cx: &mut Context<Self>) {
        let Some(pane) = self.chats.pane_mut(pane_id) else {
            return;
        };
        let Some(index) = pane.pages.iter().position(|page| page.key == key) else {
            return;
        };
        let page = pane.pages.remove(index);
        pane.remeasure(key.entry);
        #[cfg(any(target_os = "macos", windows))]
        self.close_chat_page(page.id);
        #[cfg(not(any(target_os = "macos", windows)))]
        let _ = page;
        cx.notify();
    }

    /// The chat pages to show this frame: those of chats on screen that the
    /// last frame drew, and on Windows drew whole.
    #[cfg(any(target_os = "macos", windows))]
    pub(crate) fn shown_chat_pages(&self) -> Vec<TabId> {
        let shown: Vec<TabId> = self
            .chat_pages_on_screen()
            .filter(|id| {
                self.chat_page_drawn(*id)
                    .is_some_and(|drawn| crate::browser::chat_page_visible(drawn, PAGE_HEIGHT))
            })
            .collect();
        self.chats.presented.replace(shown.clone());
        shown
    }

    /// The pages of the chats the frame on screen draws.
    #[cfg(any(target_os = "macos", windows))]
    pub(crate) fn chat_pages_on_screen(&self) -> impl Iterator<Item = TabId> + '_ {
        self.live
            .surface
            .as_ref()
            .filter(|surface| surface.popup.is_none())
            .into_iter()
            .flat_map(|surface| surface.panes.iter())
            .filter(|pane| !self.chats.raw(&pane.pane_id))
            .filter_map(|pane| self.chats.pane(&pane.pane_id))
            .filter(|chat| chat.available())
            .flat_map(|chat| chat.pages.iter().map(|page| page.id))
    }
}

/// Bytes of an HTML file shown; a larger one is refused.
const MAX_FILE: u64 = 4 * 1024 * 1024;

/// The HTML file at `path`, when it is a bounded regular file.
fn read_page(path: &Path) -> std::io::Result<Vec<u8>> {
    use std::io::Read as _;
    let mut html = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_FILE + 1)
        .read_to_end(&mut html)?;
    if html.len() as u64 > MAX_FILE {
        return Err(std::io::Error::other("the page is too large"));
    }
    Ok(html)
}

/// Writes a page to `index.html` in a folder of its own, named by what it
/// holds, and returns the file. Blocks, so it runs off the UI thread.
fn write_page(html: &[u8]) -> std::io::Result<PathBuf> {
    let root =
        private_folder("pages").ok_or_else(|| std::io::Error::other("no folder for pages"))?;
    let mut hasher = DefaultHasher::new();
    html.hash(&mut hasher);
    let folder = root.join(format!("{:016x}", hasher.finish()));
    private_dir(&root)?;
    private_dir(&folder)?;
    let file = folder.join("index.html");
    std::fs::write(&file, html)?;
    Ok(file)
}

/// The per-user folder for chat files of one `kind`, such as pages. A local
/// page may not be served from a folder with a hidden name in its path, so
/// on macOS and Windows it is the user's own temporary folder.
#[cfg(any(target_os = "macos", windows))]
pub(super) fn private_folder(kind: &str) -> Option<PathBuf> {
    Some(std::env::temp_dir().join(format!("herdr-chat-{kind}")))
}

/// Elsewhere `/tmp` is shared with other users, so the page goes in the
/// user's runtime or cache folder.
#[cfg(not(any(target_os = "macos", windows)))]
pub(super) fn private_folder(kind: &str) -> Option<PathBuf> {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .or_else(|| std::env::var_os("XDG_CACHE_HOME"))
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))?;
    base.is_absolute()
        .then(|| base.join(format!("herdr-gpui-chat-{kind}")))
}

/// Creates `path`, readable by this user alone where the platform says so.
pub(super) fn private_dir(path: &Path) -> std::io::Result<()> {
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(path)
}

#[cfg(test)]
mod tests;
