use super::*;

fn legacy(code: ClientKeyCode, modifiers: u8) -> String {
    bytes(&key(code, modifiers), &Modes::default())
}

#[test]
fn editing_keys() {
    assert_eq!(legacy(ClientKeyCode::Enter, 0), "\r");
    assert_eq!(legacy(ClientKeyCode::Enter, ALT), "\x1b\r");
    assert_eq!(legacy(ClientKeyCode::Tab, 0), "\t");
    assert_eq!(legacy(ClientKeyCode::Tab, SHIFT), "\x1b[Z");
    assert_eq!(legacy(ClientKeyCode::BackTab, 0), "\x1b[Z");
    assert_eq!(legacy(ClientKeyCode::BackTab, CONTROL), "\x1b[1;6Z");
    assert_eq!(legacy(ClientKeyCode::Backspace, 0), "\x7f");
    assert_eq!(legacy(ClientKeyCode::Backspace, CONTROL), "\x08");
    assert_eq!(legacy(ClientKeyCode::Backspace, ALT), "\x1b\x7f");
    assert_eq!(legacy(ClientKeyCode::Esc, 0), "\x1b");
    assert_eq!(legacy(ClientKeyCode::Null, 0), "\0");
    let lnm = Modes {
        line_feed_new_line: true,
        ..Modes::default()
    };
    assert_eq!(bytes(&key(ClientKeyCode::Enter, 0), &lnm), "\r\n");
}

#[test]
fn cursor_keys_follow_application_cursor_mode() {
    let app = Modes {
        app_cursor: true,
        ..Modes::default()
    };
    for (code, letter) in [
        (ClientKeyCode::Up, 'A'),
        (ClientKeyCode::Down, 'B'),
        (ClientKeyCode::Right, 'C'),
        (ClientKeyCode::Left, 'D'),
        (ClientKeyCode::Home, 'H'),
        (ClientKeyCode::End, 'F'),
    ] {
        assert_eq!(legacy(code.clone(), 0), format!("\x1b[{letter}"));
        assert_eq!(bytes(&key(code.clone(), 0), &app), format!("\x1bO{letter}"));
        // Modified keys use the CSI form in either mode.
        assert_eq!(
            bytes(&key(code, CONTROL), &app),
            format!("\x1b[1;5{letter}")
        );
    }
}

#[test]
fn modifiers_use_xterm_parameters() {
    assert_eq!(legacy(ClientKeyCode::Up, SHIFT), "\x1b[1;2A");
    assert_eq!(legacy(ClientKeyCode::Up, ALT), "\x1b[1;3A");
    assert_eq!(legacy(ClientKeyCode::Up, SHIFT | ALT), "\x1b[1;4A");
    assert_eq!(legacy(ClientKeyCode::Up, CONTROL), "\x1b[1;5A");
    assert_eq!(
        legacy(ClientKeyCode::Up, CONTROL | SHIFT | ALT),
        "\x1b[1;8A"
    );
    assert_eq!(legacy(ClientKeyCode::Up, SUPER), "\x1b[1;9A");
}

#[test]
fn tilde_keys() {
    assert_eq!(legacy(ClientKeyCode::Insert, 0), "\x1b[2~");
    assert_eq!(legacy(ClientKeyCode::Delete, 0), "\x1b[3~");
    assert_eq!(legacy(ClientKeyCode::PageUp, 0), "\x1b[5~");
    assert_eq!(legacy(ClientKeyCode::PageDown, SHIFT), "\x1b[6;2~");
}

#[test]
fn function_keys() {
    assert_eq!(legacy(ClientKeyCode::F(1), 0), "\x1bOP");
    assert_eq!(legacy(ClientKeyCode::F(4), 0), "\x1bOS");
    assert_eq!(legacy(ClientKeyCode::F(2), CONTROL), "\x1b[1;5Q");
    let tilde = [15, 17, 18, 19, 20, 21, 23, 24];
    for (number, code) in (5..=12).zip(tilde) {
        assert_eq!(legacy(ClientKeyCode::F(number), 0), format!("\x1b[{code}~"));
    }
    assert_eq!(legacy(ClientKeyCode::F(5), SHIFT), "\x1b[15;2~");
    assert_eq!(legacy(ClientKeyCode::F(0), 0), "");
    assert_eq!(legacy(ClientKeyCode::F(99), 0), "");
}

#[test]
fn control_letters_become_c0() {
    assert_eq!(legacy(ClientKeyCode::Char('c'), CONTROL), "\x03");
    assert_eq!(legacy(ClientKeyCode::Char('C'), CONTROL | SHIFT), "\x03");
    assert_eq!(legacy(ClientKeyCode::Char('a'), CONTROL | ALT), "\x1b\x01");
    assert_eq!(legacy(ClientKeyCode::Char('['), CONTROL), "\x1b");
    assert_eq!(legacy(ClientKeyCode::Char(' '), CONTROL), "\0");
    assert_eq!(legacy(ClientKeyCode::Char('?'), CONTROL), "\x7f");
    assert_eq!(legacy(ClientKeyCode::Char('é'), CONTROL), "é");
}

#[test]
fn characters_with_alt_and_shift() {
    assert_eq!(legacy(ClientKeyCode::Char('x'), ALT), "\x1bx");
    assert_eq!(legacy(ClientKeyCode::Char('x'), SHIFT), "X");
    assert_eq!(legacy(ClientKeyCode::Char('x'), SHIFT | ALT), "\x1bX");
    let ClientPaneInputEvent::Key {
        code,
        modifiers,
        kind,
        ..
    } = key(ClientKeyCode::Char('1'), SHIFT)
    else {
        unreachable!()
    };
    let shifted = ClientPaneInputEvent::Key {
        code,
        modifiers,
        kind,
        repeat_count: 1,
        shifted_codepoint: Some(u32::from('!')),
        generated_text: Some("!".to_owned()),
        tracks_release: false,
        physical_key_id: None,
        windows_record: None,
    };
    assert_eq!(bytes(&shifted, &Modes::default()), "!");
}

#[test]
fn releases_send_nothing_and_repeats_are_bounded() {
    let release = key_event(ClientKeyCode::Char('a'), CONTROL, ClientKeyKind::Release);
    assert!(encode(&release, &Modes::default()).is_empty());
    let ClientPaneInputEvent::Key {
        code, modifiers, ..
    } = key(ClientKeyCode::Left, 0)
    else {
        unreachable!()
    };
    let repeated = ClientPaneInputEvent::Key {
        code,
        modifiers,
        kind: ClientKeyKind::Repeat,
        repeat_count: 1_000,
        shifted_codepoint: None,
        generated_text: None,
        tracks_release: false,
        physical_key_id: None,
        windows_record: None,
    };
    assert_eq!(
        encode(&repeated, &Modes::default()),
        b"\x1b[D".repeat(usize::from(MAX_REPEAT))
    );
}

#[test]
fn win32_input_mode_sends_records() {
    let record = WindowsKeyRecord {
        key_down: true,
        repeat_count: 1,
        virtual_key_code: 0x41,
        virtual_scan_code: 0x1e,
        unicode: 0x61,
        control_key_state: 0x10,
    };
    let modes = Modes {
        win32_input: true,
        ..Modes::default()
    };
    let press = with_record(key(ClientKeyCode::Char('a'), 0), record);
    assert_eq!(bytes(&press, &modes), "\x1b[65;30;97;1;16;1_");
    let release = with_record(
        key_event(ClientKeyCode::Char('a'), 0, ClientKeyKind::Release),
        WindowsKeyRecord {
            key_down: false,
            ..record
        },
    );
    assert_eq!(bytes(&release, &modes), "\x1b[65;30;97;0;16;1_");
    // Without a record, or without the mode, keys encode as usual.
    assert_eq!(bytes(&key(ClientKeyCode::Up, 0), &modes), "\x1b[A");
    assert_eq!(bytes(&press, &Modes::default()), "a");
}
