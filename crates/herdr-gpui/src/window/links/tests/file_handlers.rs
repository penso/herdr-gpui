use super::*;

/// A 20-column pane whose first row is an OSC 8 hyperlink to `target`.
fn hyperlinked(target: &str) -> Arc<PaneSurfaceFrame> {
    let mut surface = wrapped(2);
    let frame = &mut Arc::make_mut(&mut surface).frame;
    frame.hyperlinks = vec![target.to_owned()];
    for (cell, symbol) in frame
        .cells
        .iter_mut()
        .zip("notes.md".chars().chain(std::iter::repeat(' ')))
    {
        cell.symbol = symbol.to_string();
        cell.hyperlink = None;
    }
    for cell in &mut frame.cells[..8] {
        cell.hyperlink = Some(0);
    }
    surface
}

/// Shows a hyperlink to `document`. Its file opens in the system's
/// application, which the test observes, rather than the terminal editor:
/// these tests are about which clicks reach the daemon.
fn show(view: &mut HerdrWindow, document: &Document) {
    view.live.surface = Some(hyperlinked(&document.url()));
    view.config.open_files_in = crate::config::FileTarget::System;
}

/// A Markdown file in a directory of its own, removed when dropped.
struct Document(std::path::PathBuf);

impl Document {
    fn new() -> Self {
        static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "hg-file-handler-{}-{}",
            std::process::id(),
            SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("notes.md"), "# Notes\n").unwrap();
        Self(directory)
    }

    fn url(&self) -> String {
        url::Url::from_file_path(self.0.join("notes.md"))
            .unwrap()
            .into()
    }
}

impl Drop for Document {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The next `pane.link.activate`, skipping hover resolution.
fn activation(peer: &mut MockPeer) -> serde_json::Value {
    loop {
        let request = peer.request();
        if request["method"] == "pane.link.activate" {
            return request;
        }
    }
}

fn reply(
    peer: &mut MockPeer,
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
    request: &serde_json::Value,
    response: serde_json::Value,
) {
    let id = request["id"].as_str().unwrap();
    let mut response = response;
    response["id"] = json!(id);
    let event = peer.respond(&snapshot().boot_id, id, &response);
    let links = view.read_with(cx, |view, _| {
        view.endpoints[view.selected_endpoint]
            .connection
            .links
            .clone()
    });
    assert!(links.lock().unwrap().apply(event).is_none());
    cx.update(|window, cx| view.update(cx, |view, cx| view.poll_links(window, cx)));
    cx.run_until_parked();
}

/// A link-modifier click on a `file://` hyperlink reaches the daemon at once,
/// before any hover resolved it, so a plugin handler such as a Markdown
/// reviewer can claim it. A handler that fails spends the click, and only a
/// click nothing claims opens the file here.
#[gpui::test]
fn file_hyperlinks_go_to_plugin_handlers_before_opening_here(cx: &mut gpui::TestAppContext) {
    let document = Document::new();
    let mut peer = MockPeer::advertising(&["pane.link.resolve", "pane.link.activate"]);
    let (view, cx) = window(&peer, true, cx);
    view.update(cx, |view, _| show(view, &document));
    let link = at(&view, cx, 3, 0);
    view.read_with(cx, |view, _| {
        assert!(view.terminal_link_at(link).is_none(), "not a web link");
        assert!(view.hovered_daemon_link().is_none());
    });

    // The handler fails: its reason is shown and nothing opens.
    cx.simulate_click(link, Modifiers::secondary_key());
    let request = activation(&mut peer);
    assert_eq!(
        request["params"],
        json!({
            "pane_id": "w1:p1", "viewport_row": 0, "col": 3,
            "content_revision": 2, "offset_from_bottom": null,
        })
    );
    reply(
        &mut peer,
        &view,
        cx,
        &request,
        json!({"error": {"code": "plugin_link_failed", "message": "review failed"}}),
    );
    assert!(cx.opened_url().is_none());
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.local_error.as_deref(),
            Some("Link handler failed: review failed")
        );
    });

    // A handler claims it: still nothing opens here.
    cx.simulate_click(link, Modifiers::secondary_key());
    let request = activation(&mut peer);
    reply(
        &mut peer,
        &view,
        cx,
        &request,
        json!({"result": {"type": "pane_link_activated", "url": document.url(), "handled": true}}),
    );
    assert!(cx.opened_url().is_none());

    // Nothing claims it: the file opens here, once.
    cx.simulate_click(link, Modifiers::secondary_key());
    let request = activation(&mut peer);
    assert!(cx.opened_url().is_none(), "the daemon answers first");
    reply(
        &mut peer,
        &view,
        cx,
        &request,
        json!({"result": {"type": "pane_link_activated", "url": document.url(), "handled": false}}),
    );
    let opened = cx.opened_url().unwrap();
    assert!(
        opened.starts_with("file://") && opened.ends_with("/notes.md"),
        "{opened}"
    );
}

/// A hover that already resolved the hyperlink changes nothing: the click
/// still carries its file, which opens once when no handler claims it.
#[gpui::test]
fn a_resolved_file_hyperlink_keeps_its_local_fallback(cx: &mut gpui::TestAppContext) {
    let document = Document::new();
    let mut peer = MockPeer::advertising(&["pane.link.resolve", "pane.link.activate"]);
    let (view, cx) = window(&peer, true, cx);
    view.update(cx, |view, _| show(view, &document));
    let link = at(&view, cx, 3, 0);
    cx.simulate_mouse_move(link, None, Modifiers::secondary_key());
    resolve_after_delay(cx);
    let request = peer.request();
    assert_eq!(request["method"], "pane.link.resolve");
    reply(
        &mut peer,
        &view,
        cx,
        &request,
        json!({"result": {"type": "pane_link_resolved", "regions": [
            {"row": 0, "start_col": 0, "end_col": 7},
        ]}}),
    );
    view.read_with(cx, |view, _| assert!(view.hovered_daemon_link().is_some()));

    cx.simulate_click(link, Modifiers::secondary_key());
    let request = activation(&mut peer);
    reply(
        &mut peer,
        &view,
        cx,
        &request,
        json!({"result": {"type": "pane_link_activated", "url": document.url(), "handled": false}}),
    );
    let opened = cx.opened_url().unwrap();
    assert!(opened.ends_with("/notes.md"), "{opened}");
    view.read_with(cx, |view, _| assert!(view.links.activating.is_none()));
}

/// A pane on an SSH host prints that host's `file://` links, which never open
/// here, yet a quick click, before any hover resolved it, still reaches the
/// daemon so a handler on that host can claim it. Declined, nothing opens.
#[gpui::test]
fn a_remote_file_hyperlink_still_reaches_plugin_handlers(cx: &mut gpui::TestAppContext) {
    let mut peer = MockPeer::advertising(&["pane.link.resolve", "pane.link.activate"]);
    let (view, cx) = window(&peer, true, cx);
    let url = "file://devbox/home/dev/notes.md";
    view.update(cx, |view, _| {
        view.live.surface = Some(hyperlinked(url));
        let selected = view.selected_endpoint;
        view.endpoints[selected].connection.target = herdr_client::ConnectTarget::Ssh {
            target: "devbox.invalid".into(),
            session: "default".into(),
        };
        assert!(view.selected_is_remote());
    });
    let link = at(&view, cx, 3, 0);
    view.read_with(cx, |view, _| {
        assert!(view.file_link_at(link).is_none(), "not this machine's file");
        assert!(view.hovered_daemon_link().is_none());
    });

    for handled in [true, false] {
        cx.simulate_click(link, Modifiers::secondary_key());
        let request = activation(&mut peer);
        assert_eq!(request["params"]["col"], 3);
        reply(
            &mut peer,
            &view,
            cx,
            &request,
            json!({"result": {"type": "pane_link_activated", "url": url, "handled": handled}}),
        );
        assert!(cx.opened_url().is_none(), "handled={handled}");
    }
    // Without the modifier the click stays the pane's, as on any hyperlink
    // this client cannot open.
    view.read_with(cx, |view, _| {
        assert!(
            view.terminal_link_press(link, Modifiers::default())
                .is_none()
        );
    });
}

/// Without `pane.link.activate` a link-modifier click opens the file here,
/// as it always has.
#[gpui::test]
fn file_hyperlinks_open_here_without_daemon_activation(cx: &mut gpui::TestAppContext) {
    let document = Document::new();
    let peer = MockPeer::new();
    let (view, cx) = window(&peer, false, cx);
    view.update(cx, |view, _| show(view, &document));
    let link = at(&view, cx, 3, 0);
    cx.simulate_click(link, Modifiers::secondary_key());
    cx.run_until_parked();
    let opened = cx.opened_url().unwrap();
    assert!(opened.ends_with("/notes.md"), "{opened}");
}
