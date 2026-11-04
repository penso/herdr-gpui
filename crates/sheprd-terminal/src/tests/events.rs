use super::*;

#[test]
fn title_changes_coalesce_to_the_latest() {
    let mut emulator = fed(5, 1, b"\x1b]0;one\x07\x1b]2;two\x1b\\");
    assert_eq!(emulator.title(), Some("two"));
    emulator.feed(b"\x1b]0;three\x07");
    assert_eq!(
        emulator.take_events(),
        [Event::Title(Some("three".to_owned()))]
    );
    assert!(emulator.take_events().is_empty());
}

#[test]
fn setting_the_same_title_again_is_no_change() {
    let mut emulator = fed(5, 1, b"\x1b]0;same\x07");
    let _ = emulator.take_events();
    emulator.feed(b"\x1b]0;other\x07\x1b]0;same\x07");
    assert!(emulator.take_events().is_empty());
}

#[test]
fn bells_are_counted() {
    let mut emulator = fed(5, 1, b"\x07a\x07\x07");
    assert_eq!(emulator.take_events(), [Event::Bell { count: 3 }]);
    assert!(emulator.take_events().is_empty());
}

#[test]
fn queries_and_clipboard_requests_are_ignored() {
    // Device attributes, cursor position, OSC 52 store and load.
    let mut emulator = fed(5, 1, b"\x1b[c\x1b[6n\x1b]52;c;aGk=\x07\x1b]52;c;?\x07");
    assert!(emulator.take_events().is_empty());
    assert_eq!(line(&emulator.frame(), 0), "");
}

#[test]
fn synchronized_updates_wait_for_their_end() {
    let mut emulator = fed(5, 1, b"\x1b[?2026hheld");
    assert!(emulator.sync_pending());
    assert_eq!(line(&emulator.frame(), 0), "");
    emulator.end_sync();
    assert!(!emulator.sync_pending());
    assert_eq!(line(&emulator.frame(), 0), "held");
    let mut ended = fed(5, 1, b"\x1b[?2026hdone\x1b[?2026l");
    assert!(!ended.sync_pending());
    assert_eq!(line(&ended.frame(), 0), "done");
    ended.end_sync();
}

#[test]
fn modes_follow_the_program() {
    let modes = fed(
        5,
        1,
        b"\x1b[?1h\x1b=\x1b[?2004h\x1b[?1004h\x1b[?1003h\x1b[?1006h\x1b[20h\x1b[>5u",
    )
    .modes();
    assert!(modes.app_cursor && modes.app_keypad && modes.bracketed_paste);
    assert!(modes.focus_reporting && modes.line_feed_new_line);
    assert_eq!(modes.mouse, MouseReporting::Motion);
    assert_eq!(modes.mouse_encoding, MouseEncoding::Sgr);
    assert_eq!(modes.kitty_keyboard, 5);
    let modes = fed(5, 1, b"\x1b[?1000h\x1b[?1005h").modes();
    assert_eq!(
        (modes.mouse, modes.mouse_encoding),
        (MouseReporting::Click, MouseEncoding::Utf8)
    );
    assert_eq!(
        fed(5, 1, b"\x1b[?1002h").modes().mouse,
        MouseReporting::Drag
    );
    assert_eq!(emulator(5, 1).modes().mouse, MouseReporting::Off);
}
