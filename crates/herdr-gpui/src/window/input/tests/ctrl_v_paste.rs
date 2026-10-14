use super::*;

fn key(keystroke: &str) -> Keystroke {
    Keystroke::parse(keystroke).unwrap()
}

fn text() -> Option<ClipboardItem> {
    Some(ClipboardItem::new_string("clipboard text".into()))
}

#[test]
fn ctrl_v_pastes_clipboard_text_where_it_is_the_paste_key() {
    assert_eq!(plain_ctrl_v_paste(&key("ctrl-v"), true, text), text());
}

#[test]
fn ctrl_v_without_clipboard_text_stays_the_pane_key() {
    let image =
        ClipboardItem::new_image(&gpui::Image::from_bytes(gpui::ImageFormat::Png, vec![42]));
    for clipboard in [
        None,
        Some(ClipboardItem::new_string(String::new())),
        Some(image),
    ] {
        assert_eq!(
            plain_ctrl_v_paste(&key("ctrl-v"), true, || clipboard.clone()),
            None,
            "{clipboard:?}"
        );
    }
}

#[test]
fn ctrl_v_stays_the_pane_key_where_it_is_not_the_paste_key() {
    let mut read = false;
    let paste = plain_ctrl_v_paste(&key("ctrl-v"), false, || {
        read = true;
        text()
    });
    assert_eq!(paste, None);
    assert!(!read, "the clipboard is read only for the paste keystroke");
}

#[test]
fn other_chords_never_read_the_clipboard() {
    for ctrl_v_pastes in [false, true] {
        for chord in [
            "ctrl-shift-v",
            "alt-v",
            "ctrl-alt-v",
            "cmd-v",
            "ctrl-cmd-v",
            "v",
            "shift-v",
            "ctrl-c",
        ] {
            let mut read = false;
            let paste = plain_ctrl_v_paste(&key(chord), ctrl_v_pastes, || {
                read = true;
                text()
            });
            assert_eq!(paste, None, "{chord}");
            assert!(!read, "{chord}");
        }
    }
}
