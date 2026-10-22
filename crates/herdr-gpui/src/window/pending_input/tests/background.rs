//! A held paste whose clipboard is read or prepared in the background holds
//! the input made after it until that work ends: sent in order once it is
//! queued, discarded when it fails.

use super::*;
use gpui::{Image, ImageFormat};

const CTRL_V: &str = "ctrl-v";

/// Ctrl-V on the remote pane, as its key handler makes it: an image on the
/// clipboard is bridged, anything else sends the key.
fn ctrl_v(view: &mut HerdrWindow, cx: &mut Context<HerdrWindow>) {
    view.paste_or_hold(true, Some(text(CTRL_V)), cx);
}

/// A clipboard image the bridge cannot prepare.
fn unpreparable_image(cx: &mut Context<HerdrWindow>) {
    let image = Image::from_bytes(ImageFormat::Svg, b"<svg/>".to_vec());
    cx.write_to_clipboard(ClipboardItem::new_image(&image));
}

#[gpui::test]
fn a_key_held_after_a_preparing_paste_stays_held(cx: &mut TestAppContext) {
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("pasted".into()));
        sized_by_another(&peer, view, cx);
        ctrl_v(view, cx);
        view.send(text("a"), cx);
        frame_returns(view, cx);
        assert_eq!(view.pending_input.len(), 1, "the key waits for the paste");
        assert!(view.local_error.is_none(), "{:?}", view.local_error);
    });
    cx.run_until_parked();
    assert_eq!(next_pane_input(&mut peer), typed(text(CTRL_V)));
    assert_eq!(next_pane_input(&mut peer), typed(text("a")));
}

/// The paste is queued while the frame is gone again: the key behind it
/// waits for the frame, then follows it.
#[gpui::test]
fn a_key_held_after_a_prepared_paste_follows_it_once_the_frame_is_ready(cx: &mut TestAppContext) {
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("pasted".into()));
        sized_by_another(&peer, view, cx);
        ctrl_v(view, cx);
        view.send(text("a"), cx);
        frame_returns(view, cx);
        size_by_another(view);
    });
    cx.run_until_parked();
    view.update(cx, |view, cx| {
        assert_eq!(view.pending_input.len(), 1, "the key waits for the frame");
        frame_returns(view, cx);
        assert_eq!(view.pending_input.len(), 0);
    });
    assert_eq!(next_pane_input(&mut peer), typed(text(CTRL_V)));
    assert_eq!(next_pane_input(&mut peer), typed(text("a")));
    sentinel_is_next(&mut peer);
}

/// Enter held after the paste, and a key typed once the frame returned, are
/// not sent without the image.
#[gpui::test]
fn input_after_a_paste_whose_preparation_fails_is_discarded(cx: &mut TestAppContext) {
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, cx| {
        unpreparable_image(cx);
        sized_by_another(&peer, view, cx);
        ctrl_v(view, cx);
        view.send(crate::menu::enter_key(), cx);
        frame_returns(view, cx);
        view.send(text("b"), cx);
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        let error = view.local_error.as_deref().unwrap_or_default();
        assert!(error.starts_with("Image not sent: "), "{error:?}");
        assert!(error.ends_with(" (2 held inputs discarded)"), "{error:?}");
        assert_eq!(view.pending_input.len(), 0);
    });
    sentinel_is_next(&mut peer);
}

/// The mark where a fifth paste was refused discards only what the gap held:
/// a key made once the frame returned waits behind the preparing pastes and
/// follows them.
#[gpui::test]
fn a_key_made_after_the_frame_returns_passes_a_refused_paste_mark(cx: &mut TestAppContext) {
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("pasted".into()));
        sized_by_another(&peer, view, cx);
        for _ in 0..5 {
            ctrl_v(view, cx);
        }
        assert_eq!(view.local_error.as_deref(), Some(STOPPED));
        view.local_error = None;
        frame_returns(view, cx);
        view.send(text("a"), cx);
    });
    cx.run_until_parked();
    // Showing the refusal rendered the window, which sized it to the test
    // window: the frame answering that size is stood in for here.
    view.update(cx, |view, cx| {
        let frame = &view.live.surface.as_ref().unwrap().frame;
        view.options.surface_size.cols = frame.width;
        view.options.surface_size.rows = frame.height;
        assert!(view.flush_pending_input(cx));
    });
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert!(view.local_error.is_none(), "{:?}", view.local_error);
        assert_eq!(view.pending_input.len(), 0);
    });
    for expected in [CTRL_V, CTRL_V, CTRL_V, CTRL_V, "a"] {
        assert_eq!(next_pane_input(&mut peer), typed(text(expected)));
    }
    sentinel_is_next(&mut peer);
}

/// Keys and a paste made while held input waits on a preparing paste join
/// the back of the queue, and each preparing paste holds what follows it.
#[gpui::test]
fn input_made_behind_a_preparing_paste_joins_the_queue(cx: &mut TestAppContext) {
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, cx| {
        cx.write_to_clipboard(ClipboardItem::new_string("pasted".into()));
        sized_by_another(&peer, view, cx);
        ctrl_v(view, cx);
        frame_returns(view, cx);
        view.send(text("a"), cx);
        ctrl_v(view, cx);
        view.send(text("b"), cx);
        assert_eq!(view.pending_input.len(), 3, "the new input is held");
    });
    cx.run_until_parked();
    for expected in [CTRL_V, "a", CTRL_V, "b"] {
        assert_eq!(next_pane_input(&mut peer), typed(text(expected)));
    }
    sentinel_is_next(&mut peer);
    view.read_with(cx, |view, _| assert!(view.local_error.is_none()));
}
