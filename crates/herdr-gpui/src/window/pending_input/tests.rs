//! Held input stops at a paste or drop that did not go out whole, and input
//! made once the frame returns waits for the input held before it.

#![allow(clippy::unwrap_used)]

mod background;

use super::DropAt;
use crate::{
    menu::Page,
    sidebar::layout_tests::fixture_window,
    terminal::InputTarget,
    window::{HerdrWindow, MockPeer},
};
use gpui::{
    ClipboardEntry, ClipboardItem, Context, ExternalPaths, TestAppContext, Window, point, px,
};
use herdr_client::{
    ConnectTarget,
    protocol::{ClientMessage, ClientPaneInputEvent, MAX_FRAME_SIZE},
};
use std::path::PathBuf;

const BOOT: &str = "boot-v1";
const PANE: &str = "w1:p1";
const STOPPED: &str = "Input not sent: the terminal stopped updating";

/// Connect the window to `peer` and record its focused pane.
fn connect(peer: &MockPeer, view: &mut HerdrWindow, cx: &mut Context<HerdrWindow>) {
    peer.prepare(view, cx);
    assert!(view.flush_pending_input(cx), "the focused pane is recorded");
}

/// Another client sizes the frame, so input is held.
fn size_by_another(view: &mut HerdrWindow) {
    view.options.surface_size.cols += 1;
    assert!(!view.input_ready());
}

fn sized_by_another(peer: &MockPeer, view: &mut HerdrWindow, cx: &mut Context<HerdrWindow>) {
    connect(peer, view, cx);
    size_by_another(view);
}

/// The frame matches the window again, and held input is delivered.
fn frame_returns(view: &mut HerdrWindow, cx: &mut Context<HerdrWindow>) {
    view.options.surface_size.cols -= 1;
    assert!(view.flush_pending_input(cx));
}

fn hold_drop(view: &mut HerdrWindow, paths: &[&str], cx: &mut Context<HerdrWindow>) {
    let paths = paths.iter().map(PathBuf::from).collect();
    assert!(view.hold_drop(DropAt::Pane(InputTarget::Pane(PANE.into())), paths, cx));
}

/// Drop `paths` with the pointer over the pane, in a window that has not
/// got the focus; returns whether the drop gave it the focus.
fn live_drop(
    view: &mut HerdrWindow,
    paths: &[&str],
    window: &mut Window,
    cx: &mut Context<HerdrWindow>,
) -> bool {
    view.bounds.origin = window.mouse_position() - point(px(5.), px(5.));
    window.blur(cx);
    assert!(!view.focus.is_focused(window));
    let dropped = ExternalPaths(paths.iter().map(PathBuf::from).collect());
    view.drop_terminal_files(&dropped, window, cx);
    view.focus.is_focused(window)
}

fn text(text: &str) -> ClientPaneInputEvent {
    ClientPaneInputEvent::TextCommit(text.into())
}

fn paste(text: &str) -> ClientPaneInputEvent {
    ClientPaneInputEvent::Paste(text.into())
}

fn typed(event: ClientPaneInputEvent) -> ClientMessage {
    ClientMessage::ClientShellPaneInput {
        pane_id: PANE.into(),
        events: vec![event],
    }
}

/// The next input sent to a pane, skipping the resize and focus traffic.
fn next_pane_input(peer: &mut MockPeer) -> ClientMessage {
    loop {
        let message = peer.receive();
        if matches!(message, ClientMessage::ClientShellPaneInput { .. }) {
            return message;
        }
    }
}

/// Nothing the window sent reached a pane before a sentinel sent now.
fn sentinel_is_next(peer: &mut MockPeer) {
    let sentinel = text("sentinel");
    peer.client
        .handle
        .send_input(BOOT, PANE, [sentinel.clone()])
        .unwrap();
    assert_eq!(next_pane_input(peer), typed(sentinel));
}

#[gpui::test]
fn a_key_held_after_a_refused_drop_is_not_sent(cx: &mut TestAppContext) {
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, cx| {
        sized_by_another(&peer, view, cx);
        hold_drop(view, &["/tmp/bad\nname.txt"], cx);
        view.send(crate::menu::enter_key(), cx);
        assert_eq!(view.pending_input.len(), 2);
        frame_returns(view, cx);
        let expected = format!(
            "Files not pasted: {} (1 held input discarded)",
            crate::window::file_drop::Error::Control
        );
        assert_eq!(view.local_error.as_deref(), Some(expected.as_str()));
    });
    sentinel_is_next(&mut peer);
}

/// Hermetic: the copy is cancelled before the test runs its worker, and
/// `upload_files` checks cancellation before it spawns SSH (and Windows has
/// no upload at all).
#[gpui::test]
fn a_key_held_after_a_remote_copy_does_not_overtake_its_paths(cx: &mut TestAppContext) {
    const COPYING: &str = "Input not sent: the files are still copying (1 held input discarded)";
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, cx| {
        sized_by_another(&peer, view, cx);
        hold_drop(view, &["/herdr-held/a.txt", "/herdr-held/b.txt"], cx);
        view.send(crate::menu::enter_key(), cx);
        frame_returns(view, cx);
        assert!(view.file_transfer.is_some(), "the copy started");
        assert_eq!(view.local_error.as_deref(), Some(COPYING));
        view.file_transfer.as_ref().unwrap().cancel();
    });
    sentinel_is_next(&mut peer);
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.local_error.as_deref(), Some(COPYING));
    });
}

#[gpui::test]
fn a_held_key_that_cannot_be_sent_reports_the_input_discarded_after_it(cx: &mut TestAppContext) {
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    let oversized = text(&"x".repeat(MAX_FRAME_SIZE + 1));
    let error = peer
        .client
        .handle
        .send_input(BOOT, PANE, [oversized.clone()])
        .unwrap_err();
    view.update(cx, |view, cx| {
        sized_by_another(&peer, view, cx);
        view.send(oversized, cx);
        view.send(crate::menu::enter_key(), cx);
        view.send(text("b"), cx);
        assert_eq!(view.pending_input.len(), 3);
        frame_returns(view, cx);
        let expected = format!("Input not sent: {error} (2 held inputs discarded)");
        assert_eq!(view.local_error.as_deref(), Some(expected.as_str()));
    });
    sentinel_is_next(&mut peer);
}

/// The text of a paste too large for one frame is refused as it is sent.
#[cfg(not(target_os = "macos"))]
#[gpui::test]
fn a_key_held_after_a_paste_that_cannot_be_sent_is_not_sent(cx: &mut TestAppContext) {
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    let oversized = "x".repeat(MAX_FRAME_SIZE + 1);
    let error = peer
        .client
        .handle
        .send_input(BOOT, PANE, [paste(&oversized)])
        .unwrap_err();
    view.update(cx, |view, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string(oversized));
        sized_by_another(&peer, view, cx);
        view.paste(cx);
        view.send(crate::menu::enter_key(), cx);
        assert_eq!(view.pending_input.len(), 2);
        frame_returns(view, cx);
        let expected = format!("Input not sent: {error} (1 held input discarded)");
        assert_eq!(view.local_error.as_deref(), Some(expected.as_str()));
    });
    sentinel_is_next(&mut peer);
}

#[gpui::test]
fn a_key_held_after_a_paste_refused_by_an_open_menu_is_not_sent(cx: &mut TestAppContext) {
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("pasted".into()));
        sized_by_another(&peer, view, cx);
        view.paste(cx);
        view.send(crate::menu::enter_key(), cx);
        assert_eq!(view.pending_input.len(), 2);
        view.menu.page = Some(Page::About);
        frame_returns(view, cx);
        assert_eq!(
            view.local_error.as_deref(),
            Some("Paste not sent: a menu is open (1 held input discarded)")
        );
    });
    sentinel_is_next(&mut peer);
}

/// Four pastes fill the queue; the fifth is refused, and the key typed
/// after it is not sent without it.
#[cfg(not(target_os = "macos"))]
#[gpui::test]
fn a_key_held_after_a_refused_fifth_paste_is_not_sent(cx: &mut TestAppContext) {
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("pasted".into()));
        sized_by_another(&peer, view, cx);
        for _ in 0..4 {
            view.paste(cx);
        }
        assert!(view.local_error.is_none());
        view.paste(cx);
        assert_eq!(view.local_error.as_deref(), Some(STOPPED));
        view.send(crate::menu::enter_key(), cx);
        frame_returns(view, cx);
        let expected = format!("{STOPPED} (1 held input discarded)");
        assert_eq!(view.local_error.as_deref(), Some(expected.as_str()));
    });
    for _ in 0..4 {
        assert_eq!(next_pane_input(&mut peer), typed(paste("pasted")));
    }
    sentinel_is_next(&mut peer);
}

#[gpui::test]
fn a_full_hold_queue_refuses_a_key(cx: &mut TestAppContext) {
    let peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, cx| {
        sized_by_another(&peer, view, cx);
        while view.pending_input.len() < super::MAX_PENDING_EVENTS {
            assert!(view.send(text("a"), cx));
        }
        assert!(!view.send(text("a"), cx));
        assert_eq!(view.local_error.as_deref(), Some(STOPPED));
    });
}

/// An error shown before a held paste is still shown once the paste is sent.
#[gpui::test]
fn an_older_error_survives_a_held_paste_that_is_sent(cx: &mut TestAppContext) {
    let peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("pasted".into()));
        sized_by_another(&peer, view, cx);
        view.paste(cx);
        view.local_error = Some("older".into());
        frame_returns(view, cx);
        assert_eq!(view.local_error.as_deref(), Some("older"));
    });
}

/// A held drop refused while another copy runs reports its own reason with
/// the discard count, never an older error's.
#[gpui::test]
fn an_older_error_does_not_take_the_discard_note(cx: &mut TestAppContext) {
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, cx| {
        connect(&peer, view, cx);
        let running = vec![PathBuf::from("/herdr-held/running.txt")];
        view.start_file_transfer(InputTarget::Pane(PANE.into()), running, cx);
        assert!(view.file_transfer.is_some(), "a copy is running");
        size_by_another(view);
        hold_drop(view, &["/herdr-held/a.txt", "/herdr-held/b.txt"], cx);
        view.send(crate::menu::enter_key(), cx);
        view.local_error = Some("older".into());
        frame_returns(view, cx);
        assert_eq!(
            view.local_error.as_deref(),
            Some("Files not pasted: another copy is running (1 held input discarded)")
        );
        view.file_transfer.as_ref().unwrap().cancel();
    });
    sentinel_is_next(&mut peer);
    cx.run_until_parked();
}

#[gpui::test]
fn a_paste_made_once_the_frame_returns_arrives_after_the_held_keys(cx: &mut TestAppContext) {
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, cx| {
        sized_by_another(&peer, view, cx);
        // A local pane pastes copied files' paths at once, with no copy.
        view.endpoints[0].connection.target = ConnectTarget::Local;
        let copied = ExternalPaths(std::iter::once(PathBuf::from("/tmp/live.txt")).collect());
        let mut entries = vec![ClipboardEntry::ExternalPaths(copied)];
        entries.extend(ClipboardItem::new_string("live.txt".into()).into_entries());
        cx.write_to_clipboard(ClipboardItem { entries });
        view.send(text("a"), cx);
        assert_eq!(view.pending_input.len(), 1);
        // The frame matches again, and nothing has delivered the held key.
        view.options.surface_size.cols -= 1;
        view.paste(cx);
    });
    cx.run_until_parked();
    assert_eq!(next_pane_input(&mut peer), typed(text("a")));
    assert_eq!(next_pane_input(&mut peer), typed(paste("/tmp/live.txt")));
    sentinel_is_next(&mut peer);
    view.read_with(cx, |view, _| assert!(view.local_error.is_none()));
}

#[gpui::test]
fn a_drop_made_once_the_frame_returns_arrives_after_the_held_keys(cx: &mut TestAppContext) {
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            sized_by_another(&peer, view, cx);
            // A local pane pastes dropped paths at once, with no copy.
            view.endpoints[0].connection.target = ConnectTarget::Local;
            view.send(text("a"), cx);
            assert_eq!(view.pending_input.len(), 1);
            // The frame matches again, and nothing has delivered the held key.
            view.options.surface_size.cols -= 1;
            assert!(live_drop(view, &["/tmp/live drop.txt"], window, cx));
        });
    });
    assert_eq!(next_pane_input(&mut peer), typed(text("a")));
    assert_eq!(
        next_pane_input(&mut peer),
        typed(paste("/tmp/live\\ drop.txt"))
    );
    sentinel_is_next(&mut peer);
}

#[gpui::test]
fn a_drop_of_no_files_takes_no_focus(cx: &mut TestAppContext) {
    let peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            connect(&peer, view, cx);
            assert!(!live_drop(view, &[], window, cx));
        });
    });
}

/// A remote image drop is handed to the image bridge, which refuses the
/// second while the first is still being prepared; the window takes focus
/// either way.
#[gpui::test]
fn a_remote_image_drop_the_bridge_refuses_still_takes_focus(cx: &mut TestAppContext) {
    let image = if cfg!(windows) {
        r"C:\herdr-live\a.png"
    } else {
        "/herdr-live/a.png"
    };
    let peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            connect(&peer, view, cx);
            live_drop(view, &[image], window, cx);
            view.local_error = None;
            assert!(live_drop(view, &[image], window, cx));
            let error = view.local_error.as_deref().unwrap_or_default();
            assert!(error.starts_with("Image not sent: "), "{error:?}");
        });
    });
}
