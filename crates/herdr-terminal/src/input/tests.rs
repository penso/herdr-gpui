#![allow(clippy::unwrap_used, clippy::expect_used)]

mod keys;
mod kitty;
mod mouse;
mod paste;

use super::*;
use herdr_protocol::{ClientKeyCode, ClientKeyKind, WindowsKeyRecord};

pub(crate) fn key(code: ClientKeyCode, modifiers: u8) -> ClientPaneInputEvent {
    key_event(code, modifiers, ClientKeyKind::Press)
}

pub(crate) fn key_event(
    code: ClientKeyCode,
    modifiers: u8,
    kind: ClientKeyKind,
) -> ClientPaneInputEvent {
    ClientPaneInputEvent::Key {
        code,
        modifiers,
        kind,
        repeat_count: 1,
        shifted_codepoint: None,
        generated_text: None,
        tracks_release: false,
        physical_key_id: None,
        windows_record: None,
    }
}

pub(crate) fn with_record(
    input: ClientPaneInputEvent,
    record: WindowsKeyRecord,
) -> ClientPaneInputEvent {
    let ClientPaneInputEvent::Key {
        code,
        modifiers,
        kind,
        repeat_count,
        shifted_codepoint,
        generated_text,
        tracks_release,
        physical_key_id,
        ..
    } = input
    else {
        panic!("not a key");
    };
    ClientPaneInputEvent::Key {
        code,
        modifiers,
        kind,
        repeat_count,
        shifted_codepoint,
        generated_text,
        tracks_release,
        physical_key_id,
        windows_record: Some(record),
    }
}

pub(crate) fn bytes(input: &ClientPaneInputEvent, modes: &Modes) -> String {
    String::from_utf8(encode(input, modes)).unwrap()
}

#[test]
fn text_commits_pass_through() {
    let modes = Modes::default();
    let input = ClientPaneInputEvent::TextCommit("héllo".to_owned());
    assert_eq!(bytes(&input, &modes), "héllo");
}

#[test]
fn the_emulator_encodes_with_its_own_modes() {
    let mut emulator = crate::Emulator::new(crate::Size::new(5, 1).unwrap(), 0);
    let up = key(ClientKeyCode::Up, 0);
    assert_eq!(emulator.encode(&up), b"\x1b[A");
    emulator.feed(b"\x1b[?1h");
    assert_eq!(emulator.encode(&up), b"\x1bOA");
}
