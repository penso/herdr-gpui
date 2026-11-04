use super::*;

fn kitty(flags: u8) -> Modes {
    Modes {
        kitty_keyboard: flags,
        ..Modes::default()
    }
}

#[test]
fn disambiguate_reports_ambiguous_keys_as_csi_u() {
    let modes = kitty(1);
    assert_eq!(bytes(&key(ClientKeyCode::Esc, 0), &modes), "\x1b[27u");
    assert_eq!(
        bytes(&key(ClientKeyCode::Char('c'), CONTROL), &modes),
        "\x1b[99;5u"
    );
    assert_eq!(
        bytes(&key(ClientKeyCode::Char('C'), CONTROL | SHIFT), &modes),
        "\x1b[99;6u"
    );
    assert_eq!(
        bytes(&key(ClientKeyCode::Char('x'), ALT), &modes),
        "\x1b[120;3u"
    );
    assert_eq!(
        bytes(&key(ClientKeyCode::Enter, SHIFT), &modes),
        "\x1b[13;2u"
    );
    assert_eq!(bytes(&key(ClientKeyCode::BackTab, 0), &modes), "\x1b[9;2u");
    assert_eq!(
        bytes(&key(ClientKeyCode::Char('a'), SUPER | HYPER | META), &modes),
        "\x1b[97;57u"
    );
    // Plain text and plain editing keys keep their legacy bytes.
    assert_eq!(bytes(&key(ClientKeyCode::Char('a'), 0), &modes), "a");
    assert_eq!(bytes(&key(ClientKeyCode::Char('a'), SHIFT), &modes), "A");
    assert_eq!(bytes(&key(ClientKeyCode::Enter, 0), &modes), "\r");
    assert_eq!(bytes(&key(ClientKeyCode::Backspace, 0), &modes), "\x7f");
    assert_eq!(bytes(&key(ClientKeyCode::Up, 0), &modes), "\x1b[A");
    assert_eq!(bytes(&key(ClientKeyCode::Up, CONTROL), &modes), "\x1b[1;5A");
    assert_eq!(bytes(&key(ClientKeyCode::Delete, 0), &modes), "\x1b[3~");
    assert_eq!(bytes(&key(ClientKeyCode::F(5), ALT), &modes), "\x1b[15;3~");
    assert_eq!(bytes(&key(ClientKeyCode::F(13), 0), &modes), "\x1b[57376u");
}

#[test]
fn event_types_report_repeats_and_releases() {
    let modes = kitty(1 | 2);
    let release = key_event(ClientKeyCode::Char('c'), CONTROL, ClientKeyKind::Release);
    assert_eq!(bytes(&release, &modes), "\x1b[99;5:3u");
    let repeat = key_event(ClientKeyCode::Esc, 0, ClientKeyKind::Repeat);
    assert_eq!(bytes(&repeat, &modes), "\x1b[27;1:2u");
    let arrow = key_event(ClientKeyCode::Up, 0, ClientKeyKind::Release);
    assert_eq!(bytes(&arrow, &modes), "\x1b[1;1:3A");
    // Text keys report no release unless every key is an escape.
    let text = key_event(ClientKeyCode::Char('a'), 0, ClientKeyKind::Release);
    assert!(encode(&text, &modes).is_empty());
    assert!(encode(&release, &kitty(1)).is_empty());
}

#[test]
fn all_keys_as_escapes_covers_text() {
    let modes = kitty(1 | 8);
    assert_eq!(bytes(&key(ClientKeyCode::Char('a'), 0), &modes), "\x1b[97u");
    assert_eq!(
        bytes(&key(ClientKeyCode::Char('A'), SHIFT), &modes),
        "\x1b[97;2u"
    );
    assert_eq!(bytes(&key(ClientKeyCode::Enter, 0), &modes), "\x1b[13u");
    assert_eq!(bytes(&key(ClientKeyCode::Tab, 0), &modes), "\x1b[9u");
    let release = key_event(ClientKeyCode::Char('a'), 0, ClientKeyKind::Release);
    assert_eq!(bytes(&release, &kitty(1 | 2 | 8)), "\x1b[97;1:3u");
}
