//! `tab`, `code`, `devices`, `orchestrator`, `review`, and the page commands
//! of `browser`: the app's own tabs, which the `browser::Store` keeps for
//! every window.
mod arrive;
mod page;

use super::{Watches, main_windows, not_found, socket, valid_caller};
use crate::{
    HerdrWindow,
    browser::{Location, Store, Tab, TabId},
    control::protocol::{
        Call, Caller, ErrorCode, Response,
        tabs::{Reply, TabInfo, TabKind, TabOp},
    },
};
use gpui::{App, WindowHandle};

pub(super) fn serve(
    call: Call<TabOp>,
    incoming: socket::Incoming,
    watches: &mut Watches,
    cx: &mut App,
) {
    let caller = call.caller;
    if !valid_caller(&caller) {
        incoming.respond(Response::error(
            ErrorCode::InvalidRequest,
            "Invalid workspace, pane, or daemon",
        ));
        return;
    }
    match call.op {
        TabOp::List { pages } => incoming.respond(list(pages, cx)),
        TabOp::Focus { id } => incoming.respond(focus(&caller, id, cx)),
        TabOp::Close { id } => incoming.respond(close(&caller, id, cx)),
        TabOp::Navigate { id, url } => incoming.respond(page::navigate(&caller, id, &url, cx)),
        TabOp::Back { id } => incoming.respond(page::history(&caller, id, false, cx)),
        TabOp::Forward { id } => incoming.respond(page::history(&caller, id, true, cx)),
        TabOp::Text { id } => page::text(caller, id, incoming, watches, cx),
        TabOp::Screenshot { id, path } => page::screenshot(caller, id, path, incoming, watches, cx),
        TabOp::Zoom { id, change } => incoming.respond(page::zoom(&caller, id, change, cx)),
        TabOp::Code { path, line } => arrive::code(&caller, &path, line, incoming, watches, cx),
        TabOp::Edit { path, line } => arrive::edit(&caller, &path, line, incoming, watches, cx),
        TabOp::Devices => arrive::devices(&caller, incoming, watches, cx),
        TabOp::Orchestrator { repo } => arrive::orchestrator(&caller, repo, incoming, watches, cx),
        TabOp::Review => arrive::review(&caller, incoming, watches, cx),
        TabOp::Px0 => arrive::px0(&caller, incoming, watches, cx),
    }
}

/// What a tab is, as `tab list` names it. The workspace's VS Code tab is
/// its editor, whatever address it shows.
fn kind(tab: &Tab) -> TabKind {
    if tab.place.is_editor() {
        return TabKind::Editor;
    }
    match &tab.location {
        None => TabKind::Blank,
        Some(Location::Web { .. }) => TabKind::Web,
        Some(Location::Local { .. }) => TabKind::File,
        Some(Location::Review { .. }) => TabKind::Review,
        Some(Location::Orchestrator { .. }) => TabKind::Orchestrator,
        Some(Location::Code { .. }) => TabKind::Code,
        Some(Location::Devices) => TabKind::Devices,
    }
}

/// The address, file, or checkout a tab shows. The VS Code tab's address
/// carries its server's token, so it is never reported.
fn location(tab: &Tab) -> Option<String> {
    if tab.place.is_code() {
        return None;
    }
    Some(match tab.location.as_ref()? {
        Location::Web { url } => url.as_str().to_owned(),
        Location::Local { file } => file.path().display().to_string(),
        Location::Review { checkout } => checkout
            .checkout
            .clone()
            .or_else(|| checkout.repo_key.clone())?,
        Location::Orchestrator { repo } => repo.checkout.clone(),
        Location::Code { file } => file.path.clone(),
        Location::Devices => return None,
    })
}

fn info(tab: &Tab) -> TabInfo {
    TabInfo {
        id: tab.id.into(),
        kind: kind(tab),
        title: tab.title.clone(),
        location: location(tab),
        workspace: tab.workspace_id.clone(),
        host: tab.scope.as_str().to_owned(),
        opened_by: tab.origin.clone(),
    }
}

fn list(pages: bool, cx: &App) -> Response {
    let tabs = cx
        .try_global::<Store>()
        .map(|store| {
            store
                .tabs()
                .map(info)
                .filter(|tab| !pages || tab.kind.is_page())
                .collect()
        })
        .unwrap_or_default();
    Response::Tab {
        reply: Reply::Tabs { tabs },
    }
}

fn find(id: u64, cx: &App) -> Result<Tab, Response> {
    cx.try_global::<Store>()
        .and_then(|store| store.get(TabId::from(id)))
        .cloned()
        .ok_or_else(|| not_found(format!("Herdr UI has no tab {id}; `tab list` lists them")))
}

fn closed() -> Response {
    Response::error(ErrorCode::NoWindow, "The window closed")
}

fn busy() -> Response {
    Response::error(
        ErrorCode::WindowBusy,
        "A dialog is open in the window; close it first",
    )
}

/// The windows a request may act in: the one `--window` names, else all,
/// frontmost first.
fn windows(caller: &Caller, cx: &App) -> Vec<WindowHandle<HerdrWindow>> {
    let mut windows = main_windows(cx);
    if let Some(id) = caller.window {
        windows.retain(|handle| handle.window_id().as_u64() == id);
    }
    windows
}

/// A window listing the workspace `tab` belongs to, and the endpoint there.
fn showing(caller: &Caller, tab: &Tab, cx: &App) -> Option<(WindowHandle<HerdrWindow>, usize)> {
    windows(caller, cx).into_iter().find_map(|handle| {
        let index = handle
            .read_with(cx, |view, _| {
                view.endpoint_showing(&tab.scope, &tab.workspace_id)
            })
            .ok()
            .flatten()?;
        Some((handle, index))
    })
}

/// Switches a window to `tab`'s workspace and shows the tab there. Shown
/// against its workspace, the tab is in front once the switch lands.
fn reveal(caller: &Caller, tab: &Tab, cx: &mut App) -> Result<(), Response> {
    let Some((handle, index)) = showing(caller, tab, cx) else {
        return Err(not_found(format!(
            "No Herdr UI window shows workspace {}, which tab {} belongs to",
            tab.workspace_id, tab.id
        )));
    };
    handle
        .update(cx, |view, window, cx| {
            if view.menu.page.is_some() {
                return Err(busy());
            }
            view.go_to_workspace(index, &tab.workspace_id, cx);
            view.show_browser_tab(tab.id, window, cx);
            Ok(())
        })
        .unwrap_or_else(|_| Err(closed()))
}

fn focus(caller: &Caller, id: u64, cx: &mut App) -> Response {
    let tab = match find(id, cx) {
        Ok(tab) => tab,
        Err(response) => return response,
    };
    match reveal(caller, &tab, cx) {
        Ok(()) => Response::done(format!(
            "Showing tab {id} ({}) in workspace {}",
            tab.title, tab.workspace_id
        )),
        Err(response) => response,
    }
}

/// Closes a tab in the window showing its workspace, which moves its group
/// to another tab; with no such window, in the store, which every window
/// follows.
fn close(caller: &Caller, id: u64, cx: &mut App) -> Response {
    let tab = match find(id, cx) {
        Ok(tab) => tab,
        Err(response) => return response,
    };
    match showing(caller, &tab, cx) {
        Some((handle, _)) => {
            let _ = handle.update(cx, |view, window, cx| {
                view.close_browser_tab(tab.id, window, cx);
            });
        }
        // A named window must be the one showing the tab: a stale or wrong
        // `--window` never closes a tab elsewhere.
        None => {
            if let Some(window) = caller.window {
                return not_found(format!("Window {window} does not show tab {id}"));
            }
        }
    }
    Store::update(cx, |store| store.close(tab.id));
    Response::done(format!("Closed tab {id} ({})", tab.title))
}

#[cfg(test)]
mod tests;
