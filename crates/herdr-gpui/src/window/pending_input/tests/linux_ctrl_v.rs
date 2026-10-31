//! On Linux plain Ctrl+V, which desktops such as Omarchy send for Super+V,
//! pastes clipboard text, here into the mock peer's remote pane, whose
//! clipboard image is still bridged; anything else reaches the pane as the
//! Ctrl+V key. Local panes are covered by the endpoint lifecycle tests.
use super::*;
use gpui::{Image, ImageFormat};

fn ctrl_v() -> gpui::KeyDownEvent {
    gpui::KeyDownEvent {
        keystroke: gpui::Keystroke::parse("ctrl-v").unwrap(),
        is_held: false,
        prefer_character_input: false,
    }
}

/// What plain Ctrl+V sends with `clipboard` copied.
fn ctrl_v_with(clipboard: ClipboardItem, cx: &mut TestAppContext) -> ClientMessage {
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update_in(cx, |view, window, cx| {
        connect(&peer, view, cx);
        cx.write_to_clipboard(clipboard);
        view.key_down(&ctrl_v(), window, cx);
        assert!(view.local_error.is_none(), "{:?}", view.local_error);
    });
    cx.run_until_parked();
    let sent = loop {
        let message = peer.receive();
        if matches!(
            message,
            ClientMessage::ClientShellPaneInput { .. } | ClientMessage::ClipboardImage { .. }
        ) {
            break message;
        }
    };
    sentinel_is_next(&mut peer);
    sent
}

#[gpui::test]
fn ctrl_v_pastes_clipboard_text(cx: &mut TestAppContext) {
    let text = ClipboardItem::new_string("copied in a browser".into());
    assert_eq!(ctrl_v_with(text, cx), typed(paste("copied in a browser")));
}

#[gpui::test]
fn ctrl_v_bridges_a_remote_pane_s_clipboard_image(cx: &mut TestAppContext) {
    let image = Image::from_bytes(ImageFormat::Png, vec![42]);
    assert_eq!(
        ctrl_v_with(ClipboardItem::new_image(&image), cx),
        ClientMessage::ClipboardImage {
            target: herdr_client::protocol::ClientClipboardImageTarget::Pane(PANE.into()),
            extension: "png".into(),
            data: vec![42],
        }
    );
}

#[gpui::test]
fn ctrl_v_with_an_empty_clipboard_reaches_the_pane_as_the_key(cx: &mut TestAppContext) {
    let empty = ClipboardItem::new_string(String::new());
    let key = crate::terminal::key_input(&ctrl_v(), true).unwrap();
    assert_eq!(ctrl_v_with(empty, cx), typed(key));
}
