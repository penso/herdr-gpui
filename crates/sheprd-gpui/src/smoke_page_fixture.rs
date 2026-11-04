//! The page the native page checks load, served on the loopback address so
//! nothing leaves the machine, and the split the fixture opens it in.
use super::*;
use crate::browser::{GroupId, Location, Store, TabId, WebUrl};
use std::io::{Read as _, Write as _};

#[cfg(test)]
#[path = "smoke_page_fixture/tests.rs"]
mod tests;

/// The page the check loads: one element over all of it, which the pointer
/// hovers anywhere on the page.
pub(super) const PAGE: &str =
    "<!doctype html><title>Pages check</title><div id=spot style='position:fixed;inset:0'></div>";

/// Serves `PAGE` on the loopback address for as long as the check runs, and
/// returns its port.
pub(super) fn serve() -> Result<u16> {
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    let port = listener.local_addr()?.port();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let _ = respond(stream);
        }
    });
    Ok(port)
}

fn respond(mut stream: std::net::TcpStream) -> std::io::Result<()> {
    // Chromium can preconnect without sending a request. An idle socket
    // must not hold up the accept loop and the connection loading the page.
    stream.set_read_timeout(Some(Duration::from_millis(250)))?;
    stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    let mut request = [0; 1024];
    if stream.read(&mut request)? == 0 {
        return Ok(());
    }
    // Every request gets the page, so the request itself is dropped.
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{PAGE}",
        PAGE.len()
    )
}

/// The address `serve` serves its page at on `port`.
pub(super) fn page_url(port: u16) -> Result<WebUrl> {
    Ok(WebUrl::try_from(
        format!("http://127.0.0.1:{port}/").as_str(),
    )?)
}

/// Splits the fixture's one workspace and opens a tab on `url` in the right
/// group, or a blank tab with its address field focused. Returns the group
/// and the tab.
pub(super) fn open_page(
    view: &mut SheprdWindow,
    window: &mut Window,
    cx: &mut Context<SheprdWindow>,
    url: Option<WebUrl>,
) -> Result<(GroupId, TabId)> {
    window.resize(size(px(1200.), px(780.)));
    let mut shown = sidebar::layout_tests::snapshot(1);
    shown.focused_workspace_id = Some("w0".into());
    shown.focused_tab_id = Some("t0".into());
    view.live.snapshot = Some(Arc::new(shown));
    view.command(Command::SplitEditor, window, cx);
    let right = view
        .group_slots()
        .get(1)
        .map(|slot| slot.id)
        .context("the split made no second group")?;
    let scope = crate::browser::scope(&view.endpoints[view.selected_endpoint]);
    let location = url.map(|url| Location::Web { url });
    let id = Store::update(cx, |store| store.open(scope, "w0", location, None))
        .context("no browser tab opened")?;
    view.show_browser_tab_in(Some(right), id, window, cx);
    Ok((right, id))
}
