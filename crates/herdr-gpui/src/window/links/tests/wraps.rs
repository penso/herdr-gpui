use super::*;
use crate::links::LinkRegion;

/// An application's word wrap of an address the daemon reads only to the
/// end of its first row.
const REPLY: [&str; 2] = ["● see https://ex.co/", "  aaaaaaaa/b.html ok"];
const LOCAL: &str = "https://ex.co/aaaaaaaa/b.html";
const DAEMON: &str = "https://ex.co/";

/// The pane of `wrapped` showing `lines` instead.
fn showing(lines: &[&str]) -> Arc<PaneSurfaceFrame> {
    let mut surface = (*wrapped(2)).clone();
    let width = usize::from(surface.frame.width);
    for (row, cells) in surface.frame.cells.chunks_mut(width).enumerate() {
        let line = lines.get(row).copied().unwrap_or_default();
        let mut symbols = line.chars().chain(std::iter::repeat(' '));
        for cell in cells {
            cell.symbol = symbols.next().unwrap().to_string();
        }
    }
    Arc::new(surface)
}

/// A window showing `lines`, hovered with the link modifier on the address
/// on the first row, which the daemon resolved to `regions`.
fn hovered<'a>(
    peer: &MockPeer,
    lines: &[&str],
    regions: Vec<LinkRegion>,
    cx: &'a mut gpui::TestAppContext,
) -> (
    Entity<HerdrWindow>,
    &'a mut VisualTestContext,
    Point<Pixels>,
) {
    let (view, cx) = window(peer, true, cx);
    view.update(cx, |view, _| view.live.surface = Some(showing(lines)));
    let position = at(&view, cx, 8, 0);
    cx.simulate_mouse_move(position, None, Modifiers::secondary_key());
    view.update(cx, |view, _| {
        let cell = view.links.hover.clone().unwrap();
        view.links.resolved = Some(ResolvedLink { cell, regions });
        assert!(view.hovered_daemon_link().is_some());
    });
    (view, cx, position)
}

fn region(row: u16, start_col: u16, end_col: u16) -> LinkRegion {
    LinkRegion {
        row,
        start_col,
        end_col,
    }
}

#[gpui::test]
fn a_wrapped_link_hovered_on_its_second_row_is_underlined_on_both(cx: &mut gpui::TestAppContext) {
    let peer = MockPeer::new();
    let (view, cx) = window(&peer, false, cx);
    let continuation = at(&view, cx, 2, 1);
    cx.simulate_mouse_move(continuation, None, Modifiers::secondary_key());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let link = view.hovered_local_link().unwrap();
        assert_eq!(link.target, RowTarget::Web(URL.into()));
        assert_eq!(link.rows, vec![(0, 0..20), (1, 0..12)]);
        assert_eq!(view.underlined_link_rows(), link.rows);
    });
}

#[gpui::test]
fn a_local_reading_that_strictly_contains_the_daemons_is_underlined(cx: &mut gpui::TestAppContext) {
    let peer = MockPeer::advertising(&["pane.link.resolve", "pane.link.activate"]);
    let (view, cx, _) = hovered(&peer, &REPLY, vec![region(0, 6, 19)], cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.underlined_link_rows(), vec![(0, 6..20), (1, 2..17)]);
    });
}

#[gpui::test]
fn the_daemons_link_is_underlined_when_the_local_one_does_not_contain_it(
    cx: &mut gpui::TestAppContext,
) {
    let peer = MockPeer::advertising(&["pane.link.resolve", "pane.link.activate"]);
    let regions = vec![region(0, 6, 19), region(1, 0, 3)];
    let (view, cx, _) = hovered(&peer, &[REPLY[0]], regions, cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.hovered_local_link().unwrap().rows, vec![(0, 6..20)]);
        assert_eq!(view.underlined_link_rows(), vec![(0, 6..20), (1, 0..4)]);
    });
}

/// Clicks the hovered address, and has the daemon decline it with
/// `declined`.
fn decline(
    peer: &mut MockPeer,
    view: &Entity<HerdrWindow>,
    cx: &mut VisualTestContext,
    position: Point<Pixels>,
    declined: &str,
) {
    cx.simulate_click(position, Modifiers::secondary_key());
    let request = peer.request();
    assert_eq!(request["method"], "pane.link.activate");
    answer(
        peer,
        view,
        cx,
        &request,
        json!({"type": "pane_link_activated", "url": declined, "handled": false}),
    );
}

#[gpui::test]
fn a_declined_click_opens_the_local_link_that_contains_the_daemons(cx: &mut gpui::TestAppContext) {
    let mut peer = MockPeer::advertising(&["pane.link.resolve", "pane.link.activate"]);
    let (view, cx, position) = hovered(&peer, &REPLY, vec![region(0, 6, 19)], cx);
    decline(&mut peer, &view, cx, position, DAEMON);
    assert_eq!(cx.opened_url().as_deref(), Some(LOCAL));
}

#[gpui::test]
fn a_click_before_the_hover_reply_opens_the_whole_wrapped_link(cx: &mut gpui::TestAppContext) {
    let mut peer = MockPeer::advertising(&["pane.link.resolve", "pane.link.activate"]);
    let (view, cx) = window(&peer, true, cx);
    view.update(cx, |view, _| view.live.surface = Some(showing(&REPLY)));
    let position = at(&view, cx, 8, 0);
    // The click comes within the hover's delay, before anything is resolved.
    cx.simulate_mouse_move(position, None, Modifiers::secondary_key());
    view.read_with(cx, |view, _| assert!(view.hovered_daemon_link().is_none()));
    decline(&mut peer, &view, cx, position, DAEMON);
    assert_eq!(cx.opened_url().as_deref(), Some(LOCAL));
}

#[gpui::test]
fn a_declined_click_opens_the_daemons_address_otherwise(cx: &mut gpui::TestAppContext) {
    let mut peer = MockPeer::advertising(&["pane.link.resolve", "pane.link.activate"]);
    let regions = vec![region(0, 6, 19), region(1, 0, 3)];
    let (view, cx, position) = hovered(&peer, &[REPLY[0]], regions, cx);
    // The local reading there is `https://ex.co/`, which does not contain
    // the daemon's link, so the daemon's address wins.
    decline(&mut peer, &view, cx, position, "https://ex.co/other");
    assert_eq!(cx.opened_url().as_deref(), Some("https://ex.co/other"));
}
