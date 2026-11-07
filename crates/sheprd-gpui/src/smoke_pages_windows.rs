//! Native check of a Windows page, in the daemon-free fixture window. The
//! check types the page's address into a blank tab and presses Enter through
//! the window's message queue, as a user does. WebView2 builds a page through
//! callbacks that a blocking build waits for in a nested message pump, which
//! runs GPUI's queued work while the key's App update still holds the App:
//! GPUI panics and the process aborts (#572). Work is kept queued throughout,
//! as a running app's ticks keep it, so a blocking build fails here however
//! fast WebView2 is. The check passes once the page exists and has loaded.
//!
//! Pressing the key posts a window message, which has no safe binding.
//!
//! The page comes from this process, on the loopback address, so nothing
//! leaves the machine.
#![allow(unsafe_code)]

use super::page_fixture::{open_page, page_url, serve};
use super::*;
use crate::browser::{Store, TabId};
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    UI::WindowsAndMessaging::{PostMessageW, WM_KEYDOWN},
};
use wry::raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// The title of the page `serve` serves, which the tab takes once it loads.
const TITLE: &str = "Pages check";

/// How long WebView2 may take to start, build the page, and load it.
const SETTLE: Duration = Duration::from_secs(20);

/// Enter's virtual key, and its key-down message's repeat count and scan
/// code.
const VK_RETURN: usize = 0x0D;
const RETURN_DOWN: isize = 1 | (0x1C << 16);

pub(super) fn start_pages(handle: WindowHandle<SheprdWindow>, cx: &mut App) {
    EXIT_CODE.store(1, Ordering::SeqCst);
    let timer = cx.background_executor().clone();
    // GPUI work that is always queued, borrowing the App each time it runs.
    cx.spawn({
        let timer = timer.clone();
        async move |cx| {
            loop {
                timer.timer(Duration::from_millis(5)).await;
                cx.update(|_| ());
            }
        }
    })
    .detach();
    cx.spawn(async move |cx| {
        let result = async {
            let port = serve()?;
            let id = handle
                .update(cx, |view, window, cx| type_address(view, window, cx, port))
                .map_err(|error| anyhow!("{error:#}"))??;
            AnyWindowHandle::from(handle)
                .update(cx, |_, window, cx| {
                    // Key listeners belong to the last drawn dispatch tree.
                    // Paint the new address field before posting Enter, or
                    // the message can reach the previous frame's handlers.
                    window.refresh();
                    window.draw(cx).clear(cx);
                    press_enter(window)
                })
                .map_err(|error| anyhow!("{error:#}"))??;
            let started = Instant::now();
            loop {
                timer.timer(Duration::from_millis(100)).await;
                let loaded = handle
                    .update(cx, |view, window, cx| loaded(view, id, window, cx))
                    .map_err(|error| anyhow!("{error:#}"))?;
                if loaded {
                    return Ok::<_, anyhow::Error>(format!(
                        "page built and loaded in {:?}",
                        started.elapsed()
                    ));
                }
                if started.elapsed() > SETTLE {
                    let state = handle
                        .update(cx, |view, _, cx| {
                            let tab = cx.try_global::<Store>().and_then(|store| store.get(id));
                            format!(
                                "location={:?}, title={:?}, page={}",
                                tab.and_then(|tab| tab.location.as_ref()),
                                tab.map(|tab| &tab.title),
                                view.has_page(id),
                            )
                        })
                        .map_err(|error| anyhow!("{error:#}"))?;
                    bail!("the page did not load within {SETTLE:?}: {state}");
                }
            }
        }
        .await;
        match result {
            Ok(summary) => {
                eprintln!("PAGES native PASS: {summary}");
                std::process::exit(0);
            }
            Err(error) => {
                eprintln!("PAGES native FAIL: {error:#}");
                std::process::exit(1);
            }
        }
    })
    .detach();
}

/// Opens a blank tab and types the page's address into its focused field.
fn type_address(
    view: &mut SheprdWindow,
    window: &mut Window,
    cx: &mut Context<SheprdWindow>,
    port: u16,
) -> Result<TabId> {
    let (group, id) = open_page(view, window, cx, None)?;
    let url = page_url(port)?;
    view.group_address(group, cx)
        .update(cx, |input, cx| input.set_text_selected(url.as_str(), cx));
    Ok(id)
}

/// Posts Enter after the address field has drawn, to be handled once this
/// update has ended.
fn press_enter(window: &Window) -> Result<()> {
    // Window has an inherent `window_handle` of its own.
    let RawWindowHandle::Win32(handle) = HasWindowHandle::window_handle(window)?.as_raw() else {
        bail!("not a Win32 window");
    };
    // SAFETY: posting copies the message into the queue of the window, which
    // is open for this update; Windows refuses a window that has gone.
    unsafe {
        PostMessageW(
            Some(HWND(handle.hwnd.get() as _)),
            WM_KEYDOWN,
            WPARAM(VK_RETURN),
            LPARAM(RETURN_DOWN),
        )
    }?;
    Ok(())
}

/// Whether tab `id`'s page exists and its title has reached the tab. Applies
/// the page's reports first, as a window tick does. A page that cannot be
/// created never loads, and the log says why.
fn loaded(
    view: &mut SheprdWindow,
    id: TabId,
    window: &mut Window,
    cx: &mut Context<SheprdWindow>,
) -> bool {
    view.poll_browser(window, cx);
    let titled = cx
        .try_global::<Store>()
        .and_then(|store| store.get(id))
        .is_some_and(|tab| tab.title == TITLE);
    view.has_page(id) && titled
}
