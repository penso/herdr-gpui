use super::*;

/// The foreground handler reserves FIFO order and returns before acquisition
/// finishes. The test executor advances the worker explicitly, without sleeps
/// or access to the desktop clipboard.
#[gpui::test]
fn ordinary_linux_pastes_prepare_in_background_and_keep_input_order(cx: &mut TestAppContext) {
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, cx| {
        connect(&peer, view, cx);
        for value in ["first", "second"] {
            cx.write_to_clipboard(ClipboardItem::new_string(value.into()));
            view.paste(cx);
        }
        assert_eq!(
            view.pending_images.len(),
            2,
            "each paste reserves a background read"
        );
        assert!(view.send(text("after"), cx));
        assert!(view.local_error.is_none());
    });
    cx.run_until_parked();
    for expected in [paste("first"), paste("second"), text("after")] {
        assert_eq!(next_pane_input(&mut peer), typed(expected));
    }
    sentinel_is_next(&mut peer);
}

#[gpui::test]
fn a_linux_paste_is_discarded_if_its_target_changes_before_acquisition_finishes(
    cx: &mut TestAppContext,
) {
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, cx| {
        connect(&peer, view, cx);
        cx.write_to_clipboard(ClipboardItem::new_string("old target".into()));
        view.paste(cx);
        assert_eq!(view.pending_images.len(), 1);
        view.selection_epoch += 1;
        view.cancel_stale_image();
    });
    cx.run_until_parked();
    sentinel_is_next(&mut peer);
}
