//! Keys under the kitty keyboard protocol
//! (<https://sw.kovidgoyal.net/kitty/keyboard-protocol/>): disambiguation (1),
//! event types (2), and all keys as escapes (8). Alternate keys (4) and
//! associated text (16) are not reported: the app does not send the base
//! layout key, and text already arrives as text.

use super::{
    ALT, CONTROL, HYPER, META, SHIFT, SUPER,
    keys::{self, Key},
    repeats,
};
use crate::Modes;
use herdr_protocol::{ClientKeyCode, ClientKeyKind};

const DISAMBIGUATE: u8 = 1;
const EVENT_TYPES: u8 = 2;
const ALL_KEYS: u8 = 8;
/// kitty's code for F13; F14 to F35 follow it.
const F13: u32 = 57376;

pub(super) fn encode(key: &Key, modes: &Modes) -> Vec<u8> {
    let flags = modes.kitty_keyboard;
    let events = flags & EVENT_TYPES != 0;
    let all = flags & ALL_KEYS != 0;
    if key.kind == ClientKeyKind::Release && !events {
        return Vec::new();
    }
    let event = match (events, key.kind) {
        (true, ClientKeyKind::Repeat) => 2,
        (true, ClientKeyKind::Release) => 3,
        _ => 1,
    };
    let parameter = modifier(key.modifiers);
    let plain = key.modifiers & !SHIFT == 0;
    let legacy_text = !all
        && match key.code {
            ClientKeyCode::Char(_) => plain,
            ClientKeyCode::Enter | ClientKeyCode::Tab | ClientKeyCode::Backspace => {
                key.modifiers == 0
            }
            _ => false,
        };
    if legacy_text {
        // Text keys report no release unless all keys are escapes.
        if key.kind == ClientKeyKind::Release {
            return Vec::new();
        }
        return keys::legacy(key, modes).repeat(if events { 1 } else { repeats(key.repeat_count) });
    }
    let bytes = match *key.code {
        ClientKeyCode::Esc => csi_u(27, parameter, event),
        ClientKeyCode::Enter => csi_u(13, parameter, event),
        ClientKeyCode::Tab => csi_u(9, parameter, event),
        ClientKeyCode::BackTab => csi_u(9, modifier(key.modifiers | SHIFT), event),
        ClientKeyCode::Backspace => csi_u(127, parameter, event),
        ClientKeyCode::Char(c) => csi_u(u32::from(base(c)), parameter, event),
        ClientKeyCode::Up => letter(b'A', parameter, event, key, modes),
        ClientKeyCode::Down => letter(b'B', parameter, event, key, modes),
        ClientKeyCode::Right => letter(b'C', parameter, event, key, modes),
        ClientKeyCode::Left => letter(b'D', parameter, event, key, modes),
        ClientKeyCode::Home => letter(b'H', parameter, event, key, modes),
        ClientKeyCode::End => letter(b'F', parameter, event, key, modes),
        ClientKeyCode::F(number @ 1..=4) => letter(b'P' + number - 1, parameter, event, key, modes),
        ClientKeyCode::Insert => tilde(2, parameter, event),
        ClientKeyCode::Delete => tilde(3, parameter, event),
        ClientKeyCode::PageUp => tilde(5, parameter, event),
        ClientKeyCode::PageDown => tilde(6, parameter, event),
        ClientKeyCode::F(number @ 13..=35) => csi_u(F13 + u32::from(number - 13), parameter, event),
        ClientKeyCode::F(number) => match keys::function_tilde(number) {
            Some(code) => tilde(code, parameter, event),
            None => Vec::new(),
        },
        ClientKeyCode::Null if flags & DISAMBIGUATE != 0 || all => csi_u(0, parameter, event),
        ClientKeyCode::Null => keys::legacy(key, modes),
    };
    bytes.repeat(if events { 1 } else { repeats(key.repeat_count) })
}

/// kitty's modifier parameter: 1 plus shift 1, alt 2, control 4, super 8,
/// hyper 16, meta 32.
fn modifier(modifiers: u8) -> u8 {
    [
        (SHIFT, 1),
        (ALT, 2),
        (CONTROL, 4),
        (SUPER, 8),
        (HYPER, 16),
        (META, 32),
    ]
    .into_iter()
    .filter(|(bit, _)| modifiers & bit != 0)
    .fold(1, |parameter, (_, value)| parameter + value)
}

/// The unshifted key a character belongs to, which kitty reports.
fn base(c: char) -> char {
    let mut lower = c.to_lowercase();
    match (lower.next(), lower.next()) {
        (Some(lower), None) => lower,
        _ => c,
    }
}

fn fields(parameter: u8, event: u8) -> String {
    match (parameter, event) {
        (1, 1) => String::new(),
        (_, 1) => format!(";{parameter}"),
        _ => format!(";{parameter}:{event}"),
    }
}

fn csi_u(code: u32, parameter: u8, event: u8) -> Vec<u8> {
    format!("\x1b[{code}{}u", fields(parameter, event)).into_bytes()
}

fn tilde(code: u8, parameter: u8, event: u8) -> Vec<u8> {
    format!("\x1b[{code}{}~", fields(parameter, event)).into_bytes()
}

/// Cursor and F1 to F4 keys: unmodified presses keep their legacy form.
fn letter(final_byte: u8, parameter: u8, event: u8, key: &Key, modes: &Modes) -> Vec<u8> {
    if parameter == 1 && event == 1 {
        return keys::legacy(key, modes);
    }
    let mut out = format!("\x1b[1{}", fields(parameter, event)).into_bytes();
    out.push(final_byte);
    out
}
