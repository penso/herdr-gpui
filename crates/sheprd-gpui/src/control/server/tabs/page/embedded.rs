//! The page commands of builds that show pages. A page lives in a window
//! once that window has shown its tab; a command for a page no window has
//! made shows the tab first, and the answer waits for the page.
use super::{MAX_TEXT_JSON, bounded_text, page_tab};
use crate::{
    SheprdWindow,
    browser::TabId,
    control::{
        protocol::{
            Caller, ErrorCode, Response,
            tabs::{PAGE_WAIT_SECONDS, Reply},
        },
        server::{
            Watches, socket,
            tabs::{reveal, windows},
        },
    },
};
use gpui::{App, WindowHandle};
use std::{
    sync::{Arc, Mutex, PoisonError},
    time::Duration,
};

/// The window that has made tab `id`'s page.
fn holder(caller: &Caller, id: TabId, cx: &App) -> Option<WindowHandle<SheprdWindow>> {
    windows(caller, cx).into_iter().find(|handle| {
        handle
            .read_with(cx, |view, _| view.has_page(id))
            .unwrap_or(false)
    })
}

fn not_loaded(id: u64) -> Response {
    Response::error(
        ErrorCode::NotFound,
        format!("No window has loaded tab {id}'s page; `tab focus {id}` shows it"),
    )
}

pub(in crate::control::server::tabs) fn history(
    caller: &Caller,
    id: u64,
    forward: bool,
    cx: &mut App,
) -> Response {
    let tab = match page_tab(id, cx) {
        Ok(tab) => tab,
        Err(response) => return response,
    };
    let Some(handle) = holder(caller, tab.id, cx) else {
        return not_loaded(id);
    };
    let moved = handle
        .read_with(cx, |view, _| view.page_history(tab.id, forward))
        .unwrap_or(false);
    if !moved {
        return not_loaded(id);
    }
    Response::done(if forward {
        format!("Went forward in tab {id}")
    } else {
        format!("Went back in tab {id}")
    })
}

/// Reads the page's visible text once it has loaded, asking again on every
/// tick while it reports it is still loading.
pub(in crate::control::server::tabs) fn text(
    caller: Caller,
    id: u64,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let tab = match page_tab(id, cx) {
        Ok(tab) => tab,
        Err(response) => return incoming.respond(response),
    };
    if holder(&caller, tab.id, cx).is_none()
        && let Err(response) = reveal(&caller, &tab, cx)
    {
        return incoming.respond(response);
    }
    // The page answers on a platform callback, which may be any thread.
    let answer: Arc<Mutex<Option<String>>> = Arc::default();
    let mut asked = false;
    let tab_id = tab.id;
    watches.add(
        incoming,
        Duration::from_secs(PAGE_WAIT_SECONDS),
        move |cx| {
            if asked {
                let raw = answer
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .take()?;
                asked = false;
                return match serde_json::from_str::<Option<String>>(&raw) {
                    Ok(Some(text)) => Some(Response::Tab {
                        reply: Reply::Text {
                            text: bounded_text(&text, MAX_TEXT_JSON),
                        },
                    }),
                    // Still loading: asked again on the next tick.
                    Ok(None) => None,
                    Err(_) => Some(Response::error(
                        ErrorCode::Failed,
                        format!("Tab {id}'s page did not hand back its text"),
                    )),
                };
            }
            let handle = holder(&caller, tab_id, cx)?;
            let slot = answer.clone();
            let done = move |raw: String| {
                *slot.lock().unwrap_or_else(PoisonError::into_inner) = Some(raw);
            };
            match handle.read_with(cx, |view, _| view.page_text(tab_id, done)) {
                Ok(Ok(true)) => {
                    asked = true;
                    None
                }
                Ok(Ok(false)) | Err(_) => None,
                Ok(Err(error)) => Some(Response::error(ErrorCode::Failed, error.to_string())),
            }
        },
        Response::error(
            ErrorCode::Timeout,
            format!("Tab {id}'s page did not finish loading in time"),
        ),
        cx,
    );
}

#[cfg(target_os = "macos")]
pub(in crate::control::server::tabs) use picture::screenshot;

#[cfg(target_os = "macos")]
mod picture;
