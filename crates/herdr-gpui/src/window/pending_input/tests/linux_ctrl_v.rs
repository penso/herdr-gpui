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
    ctrl_v_mapped(&[], clipboard, cx)
}

/// What plain Ctrl+V sends with `clipboard` copied and `pane_keys` set.
fn ctrl_v_mapped(
    pane_keys: &[(&str, &str)],
    clipboard: ClipboardItem,
    cx: &mut TestAppContext,
) -> ClientMessage {
    let mut peer = MockPeer::new();
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update_in(cx, |view, window, cx| {
        connect(&peer, view, cx);
        let pane_keys = pane_keys
            .iter()
            .map(|(typed, sent)| ((*typed).to_owned(), (*sent).to_owned()))
            .collect();
        view.config.keybindings = crate::keymap::Keymap::with_overrides(
            &Default::default(),
            &pane_keys,
            &Default::default(),
        )
        .unwrap();
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

#[gpui::test]
fn a_ctrl_v_mapped_in_pane_keys_sends_its_mapping(cx: &mut TestAppContext) {
    let text = ClipboardItem::new_string("copied".into());
    let sent = ctrl_v_mapped(&[("ctrl-v", "ctrl-q")], text, cx);
    let ctrl_q = gpui::KeyDownEvent {
        keystroke: gpui::Keystroke::parse("ctrl-q").unwrap(),
        ..ctrl_v()
    };
    assert_eq!(
        sent,
        typed(crate::terminal::key_input(&ctrl_q, true).unwrap())
    );
}
