//! Keys in legacy xterm encoding, and dispatch to the other key protocols.

use super::{ALT, CONTROL, SHIFT, kitty, repeats, xterm_modifier};
use crate::Modes;
use herdr_protocol::{ClientKeyCode, ClientKeyKind, WindowsKeyRecord};

pub(super) struct Key<'a> {
    pub(super) code: &'a ClientKeyCode,
    pub(super) modifiers: u8,
    pub(super) kind: ClientKeyKind,
    pub(super) repeat_count: u16,
    pub(super) shifted: Option<char>,
    pub(super) text: Option<&'a str>,
    pub(super) windows: Option<&'a WindowsKeyRecord>,
}

pub(super) fn encode(key: &Key, modes: &Modes) -> Vec<u8> {
    if modes.win32_input
        && let Some(record) = key.windows
    {
        return win32(record);
    }
    if modes.kitty_keyboard != 0 {
        return kitty::encode(key, modes);
    }
    if key.kind == ClientKeyKind::Release {
        return Vec::new();
    }
    legacy(key, modes).repeat(repeats(key.repeat_count))
}

/// A Win32 input record: `CSI Vk ; Sc ; Uc ; Kd ; Cs ; Rc _`.
fn win32(record: &WindowsKeyRecord) -> Vec<u8> {
    format!(
        "\x1b[{};{};{};{};{};{}_",
        record.virtual_key_code,
        record.virtual_scan_code,
        record.unicode,
        u8::from(record.key_down),
        record.control_key_state,
        record.repeat_count,
    )
    .into_bytes()
}

/// One press in xterm's encoding.
pub(super) fn legacy(key: &Key, modes: &Modes) -> Vec<u8> {
    let modifiers = key.modifiers;
    let parameter = xterm_modifier(modifiers);
    match key.code {
        ClientKeyCode::Enter if modes.line_feed_new_line => alt_prefixed(modifiers, b"\r\n"),
        ClientKeyCode::Enter => alt_prefixed(modifiers, b"\r"),
        ClientKeyCode::Tab if modifiers & SHIFT == 0 => alt_prefixed(modifiers, b"\t"),
        ClientKeyCode::Tab | ClientKeyCode::BackTab => {
            let parameter = xterm_modifier(modifiers | SHIFT);
            if parameter == xterm_modifier(SHIFT) {
                b"\x1b[Z".to_vec()
            } else {
                csi_letter(b'Z', parameter)
            }
        }
        ClientKeyCode::Backspace if modifiers & CONTROL != 0 => alt_prefixed(modifiers, b"\x08"),
        ClientKeyCode::Backspace => alt_prefixed(modifiers, b"\x7f"),
        ClientKeyCode::Esc => alt_prefixed(modifiers, b"\x1b"),
        ClientKeyCode::Null => alt_prefixed(modifiers, b"\0"),
        ClientKeyCode::Up => cursor_key(b'A', parameter, modes),
        ClientKeyCode::Down => cursor_key(b'B', parameter, modes),
        ClientKeyCode::Right => cursor_key(b'C', parameter, modes),
        ClientKeyCode::Left => cursor_key(b'D', parameter, modes),
        ClientKeyCode::Home => cursor_key(b'H', parameter, modes),
        ClientKeyCode::End => cursor_key(b'F', parameter, modes),
        ClientKeyCode::Insert => csi_tilde(2, parameter),
        ClientKeyCode::Delete => csi_tilde(3, parameter),
        ClientKeyCode::PageUp => csi_tilde(5, parameter),
        ClientKeyCode::PageDown => csi_tilde(6, parameter),
        ClientKeyCode::F(number) => function_key(*number, parameter),
        ClientKeyCode::Char(c) => char_key(*c, key),
    }
}

fn char_key(c: char, key: &Key) -> Vec<u8> {
    let modifiers = key.modifiers;
    if modifiers & CONTROL != 0
        && let Some(byte) = control_byte(c)
    {
        return alt_prefixed(modifiers, &[byte]);
    }
    if modifiers & (CONTROL | ALT) == 0
        && let Some(text) = key.text
    {
        return text.as_bytes().to_vec();
    }
    let c = if modifiers & SHIFT != 0 {
        key.shifted.unwrap_or_else(|| shifted(c))
    } else {
        c
    };
    alt_prefixed(modifiers, c.encode_utf8(&mut [0; 4]).as_bytes())
}

fn shifted(c: char) -> char {
    let mut upper = c.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(upper), None) => upper,
        _ => c,
    }
}

/// The C0 control Ctrl with `c` sends, as xterm maps it.
pub(super) fn control_byte(c: char) -> Option<u8> {
    Some(match c {
        'a'..='z' | 'A'..='Z' => c as u8 & 0x1f,
        '@' | ' ' | '2' => 0,
        '[' | '3' => 0x1b,
        '\\' | '4' => 0x1c,
        ']' | '5' => 0x1d,
        '^' | '6' => 0x1e,
        '_' | '-' | '/' | '7' => 0x1f,
        '?' | '8' => 0x7f,
        _ => return None,
    })
}

/// Alt sends ESC before the key, as xterm's `metaSendsEscape`.
fn alt_prefixed(modifiers: u8, bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len() + 1);
    if modifiers & ALT != 0 {
        out.push(0x1b);
    }
    out.extend_from_slice(bytes);
    out
}

fn cursor_key(letter: u8, parameter: u8, modes: &Modes) -> Vec<u8> {
    match (parameter, modes.app_cursor) {
        (1, true) => vec![0x1b, b'O', letter],
        (1, false) => vec![0x1b, b'[', letter],
        _ => csi_letter(letter, parameter),
    }
}

fn function_key(number: u8, parameter: u8) -> Vec<u8> {
    match (number, function_tilde(number)) {
        (1..=4, _) if parameter == 1 => vec![0x1b, b'O', b'P' + number - 1],
        (1..=4, _) => csi_letter(b'P' + number - 1, parameter),
        (_, Some(code)) => csi_tilde(code, parameter),
        _ => Vec::new(),
    }
}

/// The `CSI n ~` number of F5 to F20.
pub(super) fn function_tilde(number: u8) -> Option<u8> {
    Some(match number {
        5 => 15,
        6 => 17,
        7 => 18,
        8 => 19,
        9 => 20,
        10 => 21,
        11 => 23,
        12 => 24,
        13 => 25,
        14 => 26,
        15 => 28,
        16 => 29,
        17 => 31,
        18 => 32,
        19 => 33,
        20 => 34,
        _ => return None,
    })
}

fn csi_letter(letter: u8, parameter: u8) -> Vec<u8> {
    let mut out = format!("\x1b[1;{parameter}").into_bytes();
    out.push(letter);
    out
}

fn csi_tilde(code: u8, parameter: u8) -> Vec<u8> {
    if parameter == 1 {
        format!("\x1b[{code}~").into_bytes()
    } else {
        format!("\x1b[{code};{parameter}~").into_bytes()
    }
}
