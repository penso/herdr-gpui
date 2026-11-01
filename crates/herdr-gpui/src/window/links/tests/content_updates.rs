use super::*;
use gpui::MouseButton;
use herdr_client::{ConnectTarget, protocol::PaneSurfaceScrollMetrics};

/// Short enough to sit on one row of the 20-column fixture, with room left
/// so the row-local detector reads it as a finished address.
const WEB: &str = "https://ex.com/a";
const OTHER: &str = "https://ex.com/b";

/// A pane whose first row is `text` and whose other cells are blank.
fn single_row(text: &str, content_revision: u64) -> Arc<PaneSurfaceFrame> {
    assert!(
        text.chars().count() < 20,
        "{text} fills the row and is not a local link"
    );
    let mut surface = wrapped(content_revision);
    let frame = &mut Arc::make_mut(&mut surface).frame;
    for (cell, symbol) in frame
        .cells
        .iter_mut()
        .zip(text.chars().chain(std::iter::repeat(' ')))
    {
        cell.symbol = symbol.to_string();
        cell.hyperlink = None;
    }
    surface
}

/// `text` as an OSC 8 hyperlink to `target` on the first row.
fn hyperlink(text: &str, target: &str, content_revision: u64) -> Arc<PaneSurfaceFrame> {
    let mut surface = single_row(text, content_revision);
    let columns = text.chars().count();
    let frame = &mut Arc::make_mut(&mut surface).frame;
    frame.hyperlinks = vec![target.to_owned()];
    for cell in frame.cells.iter_mut().take(columns) {
        cell.hyperlink = Some(0);
    }
    surface
}

fn place(view: &mut HerdrWindow, text: &str, revision: u64) {
    view.live.surface = Some(single_row(text, revision));
}

/// Press, replace the surface, release. The replacement is the whole
/// gesture's content change: nothing waits on a timer or a live agent.
fn release_after(
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
    position: Point<Pixels>,
    modifiers: Modifiers,
    between: impl FnOnce(&mut HerdrWindow),
) {
    cx.simulate_mouse_down(position, MouseButton::Left, modifiers);
    view.update(cx, |view, _| between(view));
    cx.simulate_mouse_up(position, MouseButton::Left, modifiers);
}

fn activation(peer: &mut MockPeer) -> serde_json::Value {
    loop {
        let request = peer.request();
        if request["method"] == "pane.link.activate" {
            return request;
        }
    }
}

fn handler_failed(
    peer: &mut MockPeer,
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
    request: &serde_json::Value,
) {
    let id = request["id"].as_str().unwrap();
    let event = peer.respond(
        &snapshot().boot_id,
        id,
        &json!({
            "id": id,
            "error": {"code": "plugin_link_failed", "message": "handler failed"}
        }),
    );
    let links = view.read_with(cx, |view, _| {
        view.endpoints[view.selected_endpoint]
            .connection
            .links
            .clone()
    });
    assert!(links.lock().unwrap().apply(event).is_none());
    cx.update(|window, cx| view.update(cx, |view, cx| view.poll_links(window, cx)));
}

fn declined(url: &str) -> serde_json::Value {
    json!({"type": "pane_link_activated", "url": url, "handled": false})
}

/// Press the visible address at revision 2, swap in revision 4 with the
/// address intact, and release.
fn release_on_revision(
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
    position: Point<Pixels>,
) {
    view.update(cx, |view, _| place(view, WEB, 2));
    release_after(view, cx, position, Modifiers::secondary_key(), |view| {
        place(view, WEB, 4);
    });
}

/// A redraw that leaves the address in place completes the click at the
/// release revision. A handler that claims it or fails spends the click, and
/// a decline opens the address once.
#[gpui::test]
fn an_unchanged_url_activates_at_the_release_revision(cx: &mut gpui::TestAppContext) {
    let mut peer = MockPeer::advertising(&["pane.link.resolve", "pane.link.activate"]);
    let (view, cx) = window(&peer, true, cx);
    let link = at(&view, cx, 2, 0);

    release_on_revision(&view, cx, link);
    let request = activation(&mut peer);
    assert_eq!(request["params"]["content_revision"], 4);
    assert!(
        cx.opened_url().is_none(),
        "the daemon answers before anything opens"
    );
    answer(
        &mut peer,
        &view,
        cx,
        &request,
        json!({"type": "pane_link_activated", "url": WEB, "handled": true}),
    );
    assert!(cx.opened_url().is_none());

    release_on_revision(&view, cx, link);
    let request = activation(&mut peer);
    handler_failed(&mut peer, &view, cx, &request);
    assert!(cx.opened_url().is_none());
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.local_error
                .as_deref()
                .map(crate::i18n::leak_tests::unmarked)
                .as_deref(),
            Some("Link handler failed: handler failed")
        );
        assert!(view.links.activating.is_none());
    });

    release_on_revision(&view, cx, link);
    let request = activation(&mut peer);
    assert_eq!(
        request["params"],
        json!({
            "pane_id": "w1:p1", "viewport_row": 0, "col": 2,
            "content_revision": 4, "offset_from_bottom": null,
        })
    );
    assert!(cx.opened_url().is_none());
    answer(&mut peer, &view, cx, &request, declined(WEB));
    assert_eq!(cx.opened_url().as_deref(), Some(WEB));
    view.read_with(cx, |view, _| assert!(view.links.activating.is_none()));
    cx.update(|window, cx| view.update(cx, |view, cx| view.poll_links(window, cx)));
    assert_eq!(cx.opened_url().as_deref(), Some(WEB));
    view.read_with(cx, |view, _| assert!(view.links.activating.is_none()));
}

/// A click whose revision does not move still reaches the daemon, and only a
/// decline opens the address here.
#[gpui::test]
fn an_unchanged_revision_still_opens_through_the_daemon(cx: &mut gpui::TestAppContext) {
    let mut peer = MockPeer::advertising(&["pane.link.resolve", "pane.link.activate"]);
    let (view, cx) = window(&peer, true, cx);
    view.update(cx, |view, _| place(view, WEB, 2));
    let link = at(&view, cx, 2, 0);

    cx.simulate_click(link, Modifiers::secondary_key());
    let request = activation(&mut peer);
    assert_eq!(request["params"]["content_revision"], 2);
    assert!(cx.opened_url().is_none());
    answer(
        &mut peer,
        &view,
        cx,
        &request,
        json!({"type": "pane_link_activated", "url": WEB, "handled": true}),
    );
    assert!(cx.opened_url().is_none());

    cx.simulate_click(link, Modifiers::secondary_key());
    let request = activation(&mut peer);
    handler_failed(&mut peer, &view, cx, &request);
    assert!(cx.opened_url().is_none());
    view.read_with(cx, |view, _| assert!(view.links.activating.is_none()));

    cx.simulate_click(link, Modifiers::secondary_key());
    let request = activation(&mut peer);
    assert_eq!(request["params"]["content_revision"], 2);
    answer(&mut peer, &view, cx, &request, declined(WEB));
    assert_eq!(cx.opened_url().as_deref(), Some(WEB));
}

fn assert_not_activated(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, label: &str) {
    assert!(cx.opened_url().is_none(), "{label}");
    view.read_with(cx, |view, _| {
        assert!(view.links.activating.is_none(), "{label}");
        assert!(view.pressed_terminal_link.is_none(), "{label}");
    });
}

/// Replacing or clearing the address cancels the click while the pane
/// viewport stays the one that was pressed.
#[gpui::test]
fn a_changed_or_removed_url_does_not_activate(cx: &mut gpui::TestAppContext) {
    let peer = MockPeer::advertising(&["pane.link.resolve", "pane.link.activate"]);
    let (view, cx) = window(&peer, true, cx);
    let link = at(&view, cx, 2, 0);

    view.update(cx, |view, _| place(view, WEB, 2));
    release_after(&view, cx, link, Modifiers::secondary_key(), |view| {
        place(view, OTHER, 2);
    });
    assert_not_activated(&view, cx, "changed url");
    view.read_with(cx, |view, _| {
        assert_eq!(view.terminal_link_at(link).as_deref(), Some(OTHER));
    });

    view.update(cx, |view, _| place(view, WEB, 2));
    release_after(&view, cx, link, Modifiers::secondary_key(), |view| {
        place(view, "see notes", 2);
    });
    assert_not_activated(&view, cx, "removed url");
    view.read_with(cx, |view, _| assert!(view.terminal_link_at(link).is_none()));
}

fn prepare(view: &mut HerdrWindow) {
    Arc::make_mut(view.live.snapshot.as_mut().unwrap()).boot_id = snapshot().boot_id;
    view.live.supports_link_activate = true;
    place(view, WEB, 2);
}

/// Press the address, then revise it and apply `change`, which keeps the
/// address readable. The click must not activate.
fn reject_revised(
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
    link: Point<Pixels>,
    label: &str,
    change: impl FnOnce(&mut HerdrWindow),
) {
    view.update(cx, |view, _| prepare(view));
    release_after(view, cx, link, Modifiers::secondary_key(), |view| {
        place(view, WEB, 4);
        change(view);
    });
    assert_not_activated(view, cx, label);
    view.read_with(cx, |view, _| {
        assert_eq!(view.terminal_link_at(link).as_deref(), Some(WEB), "{label}");
    });
}

/// The address can survive a redraw and still be a different click: another
/// boot, pane, rectangle, or scroll offset, or a daemon cell on only one
/// side of the gesture.
#[gpui::test]
fn a_different_viewport_or_a_missing_cell_does_not_activate(cx: &mut gpui::TestAppContext) {
    let peer = MockPeer::advertising(&["pane.link.resolve", "pane.link.activate"]);
    let (view, cx) = window(&peer, true, cx);
    let link = at(&view, cx, 2, 0);
    reject_revised(&view, cx, link, "boot", |view| {
        let boot = "next-boot";
        Arc::make_mut(view.live.snapshot.as_mut().unwrap()).boot_id = boot.into();
        Arc::make_mut(view.live.surface.as_mut().unwrap()).boot_id = boot.into();
    });
    reject_revised(&view, cx, link, "pane", |view| {
        Arc::make_mut(view.live.surface.as_mut().unwrap()).panes[0].pane_id = "other".into();
    });
    reject_revised(&view, cx, link, "rectangle", |view| {
        Arc::make_mut(view.live.surface.as_mut().unwrap()).panes[0]
            .inner_rect
            .height = 3;
    });
    reject_revised(&view, cx, link, "scroll", |view| {
        Arc::make_mut(view.live.surface.as_mut().unwrap()).panes[0].scroll =
            Some(PaneSurfaceScrollMetrics {
                offset_from_bottom: 4,
                max_offset_from_bottom: 10,
                viewport_rows: 4,
            });
    });

    // The press had a daemon cell; the release, at the new revision, does not.
    view.update(cx, |view, _| prepare(view));
    release_after(&view, cx, link, Modifiers::secondary_key(), |view| {
        place(view, WEB, 4);
        view.live.supports_link_activate = false;
    });
    assert_not_activated(&view, cx, "release cell missing");
    view.read_with(cx, |view, _| {
        assert_eq!(view.terminal_link_at(link).as_deref(), Some(WEB));
        assert!(
            view.terminal_link_press(link, Modifiers::secondary_key())
                .is_some_and(|pressed| pressed.cell.is_none())
        );
    });

    // The release grew a daemon cell the press did not have.
    view.update(cx, |view, _| {
        prepare(view);
        view.live.supports_link_activate = false;
    });
    release_after(&view, cx, link, Modifiers::secondary_key(), |view| {
        view.live.supports_link_activate = true;
        place(view, WEB, 4);
    });
    assert_not_activated(&view, cx, "press cell missing");
    view.read_with(cx, |view, _| {
        assert!(
            view.terminal_link_press(link, Modifiers::secondary_key())
                .is_some_and(
                    |pressed| pressed.cell.is_some() && pressed.url.as_deref() == Some(WEB)
                )
        );
    });
}

/// Retargeting a device session clears the pending press, so a release on
/// an identical-looking address does not inherit it.
#[gpui::test]
fn endpoint_retargeting_drops_a_pending_press(cx: &mut gpui::TestAppContext) {
    let peer = MockPeer::advertising(&["pane.link.resolve", "pane.link.activate"]);
    let (view, cx) = window(&peer, true, cx);
    view.update(cx, |view, _| {
        place(view, WEB, 2);
        view.endpoints[0].live.supports_surface = true;
        view.endpoints[0].live.status = ConnectionStatus::Connected;
        let mut remote = crate::endpoint::Endpoint::new(
            "ssh:box".into(),
            "Box".into(),
            ConnectTarget::Ssh {
                target: "user@box".into(),
                session: "default".into(),
            },
            true,
        );
        remote.live.supports_surface = true;
        view.endpoints.push(remote);
    });
    let link = at(&view, cx, 2, 0);
    cx.simulate_mouse_down(link, MouseButton::Left, Modifiers::secondary_key());
    let (surface, snapshot) = view.read_with(cx, |view, _| {
        assert!(view.pressed_terminal_link.is_some());
        (view.live.surface.clone(), view.live.snapshot.clone())
    });
    view.update(cx, |view, cx| {
        view.select_device_session("ssh:box", "other", cx);
        assert!(view.pressed_terminal_link.is_none());
        view.live.surface = surface;
        view.live.snapshot = snapshot;
        view.live.status = ConnectionStatus::Connected;
        view.live.supports_link_activate = true;
    });
    cx.simulate_mouse_up(link, MouseButton::Left, Modifiers::secondary_key());
    assert_not_activated(&view, cx, "retarget");
    view.read_with(cx, |view, _| {
        assert_eq!(view.terminal_link_at(link).as_deref(), Some(WEB));
    });
}

/// File paths and links with no locally known address keep rejecting a
/// revision change. A resolved hover is stale after one, and an odd
/// write-in-progress revision is not a cell a click may send.
#[gpui::test]
fn file_and_daemon_only_links_reject_revision_changes(cx: &mut gpui::TestAppContext) {
    let mut peer = MockPeer::advertising(&["pane.link.resolve", "pane.link.activate"]);
    let (view, cx) = window(&peer, true, cx);
    let continuation = at(&view, cx, 2, 1);
    cx.simulate_mouse_move(continuation, None, Modifiers::secondary_key());
    resolve_after_delay(cx);
    let request = peer.request();
    assert_eq!(request["method"], "pane.link.resolve");
    answer(
        &mut peer,
        &view,
        cx,
        &request,
        json!({"type": "pane_link_resolved", "regions": [
            {"row": 0, "start_col": 0, "end_col": 19},
            {"row": 1, "start_col": 0, "end_col": 11},
        ]}),
    );
    view.read_with(cx, |view, _| assert!(view.hovered_daemon_link().is_some()));
    cx.simulate_mouse_down(continuation, MouseButton::Left, Modifiers::secondary_key());
    view.read_with(cx, |view, _| {
        let pressed = view.pressed_terminal_link.as_ref().unwrap();
        assert!(pressed.url.is_none());
        assert!(pressed.file.is_none());
        assert!(pressed.cell.is_some());
    });
    view.update(cx, |view, _| view.live.surface = Some(wrapped(4)));
    view.read_with(cx, |view, _| assert!(view.hovered_daemon_link().is_none()));
    cx.simulate_mouse_up(continuation, MouseButton::Left, Modifiers::secondary_key());
    assert_not_activated(&view, cx, "wrapped");

    let modifiers = Modifiers::secondary_key();
    let path = "/tmp/notes.md";
    view.update(cx, |view, _| place(view, path, 2));
    let link = at(&view, cx, 2, 0);
    // A printed path is a link once it is found on disk, as a hover with
    // the modifier held would have it.
    view.update(cx, |view, _| {
        let file = view.file_link_at(link).unwrap();
        view.links.checked = Some((file, Some(true)));
    });
    view.read_with(cx, |view, _| {
        assert!(view.terminal_link_at(link).is_none());
        let pressed = view.terminal_link_press(link, modifiers).unwrap();
        assert!(pressed.url.is_none() && pressed.file.is_some() && pressed.cell.is_some());
    });
    release_after(&view, cx, link, modifiers, |view| place(view, path, 4));
    assert_not_activated(&view, cx, "file");
    view.read_with(cx, |view, _| {
        let release = view.terminal_link_press(link, modifiers).unwrap();
        assert!(release.url.is_none() && release.file.is_some() && release.cell.is_some());
    });

    let target = "file://devbox/home/dev/notes.md";
    view.update(cx, |view, _| {
        view.live.surface = Some(hyperlink("notes.md", target, 2));
    });
    view.read_with(cx, |view, _| {
        assert!(view.terminal_link_at(link).is_none());
        assert!(view.file_link_at(link).is_none());
        let pressed = view.terminal_link_press(link, modifiers).unwrap();
        assert!(pressed.url.is_none() && pressed.file.is_none() && pressed.cell.is_some());
    });
    release_after(&view, cx, link, modifiers, |view| {
        view.live.surface = Some(hyperlink("notes.md", target, 4));
    });
    assert_not_activated(&view, cx, "daemon-only");
    view.read_with(cx, |view, _| {
        let release = view.terminal_link_press(link, modifiers).unwrap();
        assert!(release.url.is_none() && release.file.is_none() && release.cell.is_some());
    });

    view.update(cx, |view, _| place(view, WEB, 2));
    release_after(&view, cx, link, Modifiers::secondary_key(), |view| {
        place(view, WEB, 3)
    });
    assert_not_activated(&view, cx, "odd revision");
    view.read_with(cx, |view, _| {
        let release = view
            .terminal_link_press(link, Modifiers::secondary_key())
            .unwrap();
        assert_eq!(release.url.as_deref(), Some(WEB));
        assert!(release.cell.is_none());
    });
}
