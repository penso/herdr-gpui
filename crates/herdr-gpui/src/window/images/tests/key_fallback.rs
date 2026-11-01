//! A paste standing in for a key still sends the key when the clipboard
//! cannot be read; any other paste reports why it was not sent.
use super::*;
use crate::window::clipboard;

fn unreadable() -> crate::Result<Option<ClipboardItem>> {
    Err(Error::Clipboard(clipboard::Error::Process(
        io::ErrorKind::NotFound.into(),
    )))
}

#[test]
fn an_unreadable_clipboard_leaves_the_key_to_go_out() {
    assert!(matches!(read_or_key(unreadable(), true), Ok(None)));
}

#[test]
fn an_unreadable_clipboard_fails_a_paste_without_a_key() {
    assert!(matches!(
        read_or_key(unreadable(), false),
        Err(Error::Clipboard(clipboard::Error::Process(_)))
    ));
}

#[test]
fn a_read_clipboard_is_kept_either_way() {
    for sends_key in [false, true] {
        let item = ClipboardItem::new_string("copied".into());
        let read = read_or_key(Ok(Some(item.clone())), sends_key).unwrap();
        assert_eq!(read, Some(item));
    }
}
