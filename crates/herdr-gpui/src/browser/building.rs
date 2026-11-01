//! Building a Windows page outside the App's borrow. WebView2 builds a page
//! through callbacks, and wry's blocking build waits for them in a nested
//! message pump. That pump runs GPUI's queued work and its frames, which
//! borrow the App, while the input or tick that asked for the page still
//! holds it: GPUI panics on the second borrow and the process aborts (#572).
//! A foreground task's future runs between GPUI's updates, holding no
//! borrow, so the page is built there, with wry's asynchronous build. Its
//! remaining short waits pump with nothing borrowed.
#![allow(unsafe_code)]

use super::TabId;
use crate::HerdrWindow;
use gpui::{Context, Window};
use wry::raw_window_handle::{
    HandleError, HasWindowHandle, RawWindowHandle, Win32WindowHandle, WindowHandle,
};

/// Builds tab `id`'s page from `builder` as a child of `window`, then hands
/// it to the window, which keeps it if this is still the tab's latest build,
/// `number`.
pub(super) fn start(
    id: TabId,
    number: u64,
    builder: wry::WebViewBuilder<'static>,
    window: &mut Window,
    cx: &mut Context<HerdrWindow>,
) {
    cx.spawn_in(window, async move |this, cx| {
        // Read while the window is known to be open, in the poll in which
        // the build starts and makes its child of it.
        let Ok(parent) = cx.update(|window, _| parent(window)) else {
            return;
        };
        let built = match parent {
            Ok(parent) => builder
                .build_as_child_async(&parent)
                .await
                .map_err(crate::Error::from),
            Err(error) => Err(error),
        };
        let _ = this.update_in(cx, |this, window, cx| {
            this.page_built(id, number, built, window, cx)
        });
    })
    .detach();
}

fn parent(window: &Window) -> crate::Result<Parent> {
    // Window has an inherent `window_handle` of its own.
    match HasWindowHandle::window_handle(window)?.as_raw() {
        RawWindowHandle::Win32(handle) => Ok(Parent(handle)),
        _ => Err(wry::Error::UnsupportedWindowHandle.into()),
    }
}

/// The window's HWND, as the handle wry builds a child of. A GPUI window's
/// handle borrows the window, which an awaiting future cannot hold, and no
/// safe owned handle exists.
struct Parent(Win32WindowHandle);

impl HasWindowHandle for Parent {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        // SAFETY: the HWND was read from the open window in the poll in
        // which wry makes its child of it, before the build first waits.
        // Windows checks an HWND on every use, so a window that closes while
        // the build waits fails the build's later calls rather than being
        // reached, and takes the child with it.
        Ok(unsafe { WindowHandle::borrow_raw(self.0.into()) })
    }
}

impl HerdrWindow {
    /// Keeps tab `id`'s page from build `number` and sends it where it was
    /// last asked to go, or notes why it could not be built. A tab closed or
    /// built again meanwhile drops it.
    fn page_built(
        &mut self,
        id: TabId,
        number: u64,
        built: crate::Result<wry::WebView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(build) = self.browser.pages.finish_building(id, number) else {
            return;
        };
        match built.and_then(|view| self.browser.pages.add(id, view, window)) {
            Ok(()) => {
                self.browser.failed.remove(&id);
                if let Some(location) = build.location {
                    self.browser.pages.load(id, &location);
                }
            }
            Err(error) => {
                tracing::warn!(%error, "Cannot create a browser page");
                self.browser
                    .failed
                    .insert(id, crate::i18n::Localize::localize(&error).into());
            }
        }
        cx.notify();
    }
}
