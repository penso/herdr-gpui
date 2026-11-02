//! Encoding the app's semantic input as the bytes a program reads from its
//! PTY. herdr-core passes input through unencoded, so the client honors the
//! modes the program set: cursor keys, the kitty keyboard protocol, Win32
//! input mode, mouse reporting, and bracketed paste. Pure: modes in, bytes out.

mod keys;
mod kitty;
mod mouse;

use crate::Modes;
use herdr_protocol::ClientPaneInputEvent;

// `ClientPaneInputEvent` modifier bits, as the app builds them.
// keep-sorted start
pub const ALT: u8 = 1 << 2;
pub const CONTROL: u8 = 1 << 1;
pub const HYPER: u8 = 1 << 4;
pub const META: u8 = 1 << 5;
pub const SHIFT: u8 = 1;
pub const SUPER: u8 = 1 << 3;
// keep-sorted end

/// The most times one event repeats its bytes (key repeats, wheel lines).
pub const MAX_REPEAT: u16 = 64;

const PASTE_START: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";

/// The PTY bytes for `input` under `modes`; empty when the program would not
/// read anything for it (a key release, a click with mouse reporting off).
pub fn encode(input: &ClientPaneInputEvent, modes: &Modes) -> Vec<u8> {
    match input {
        ClientPaneInputEvent::Key {
            code,
            modifiers,
            kind,
            repeat_count,
            shifted_codepoint,
            generated_text,
            windows_record,
            ..
        } => keys::encode(
            &keys::Key {
                code,
                modifiers: *modifiers,
                kind: *kind,
                repeat_count: *repeat_count,
                shifted: shifted_codepoint.and_then(char::from_u32),
                text: generated_text.as_deref(),
                windows: windows_record.as_ref(),
            },
            modes,
        ),
        ClientPaneInputEvent::TextCommit(text) => text.as_bytes().to_vec(),
        ClientPaneInputEvent::Mouse {
            kind,
            position,
            modifiers,
            lines,
            ..
        } => mouse::encode(*kind, *position, *modifiers, *lines, modes),
        ClientPaneInputEvent::Paste(text) => paste(text, modes),
    }
}

/// Pasted text. Bracketed, every ESC is dropped so the text cannot end the
/// paste early and run as keys; otherwise newlines become carriage returns,
/// as typed Enter would send.
fn paste(text: &str, modes: &Modes) -> Vec<u8> {
    if !modes.bracketed_paste {
        return text.replace("\r\n", "\r").replace('\n', "\r").into_bytes();
    }
    let mut bytes = Vec::with_capacity(text.len() + PASTE_START.len() + PASTE_END.len());
    bytes.extend_from_slice(PASTE_START);
    bytes.extend(text.bytes().filter(|&byte| byte != 0x1b));
    bytes.extend_from_slice(PASTE_END);
    bytes
}

/// xterm's modifier parameter: 1 plus shift 1, alt 2, control 4, meta 8.
fn xterm_modifier(modifiers: u8) -> u8 {
    1 + u8::from(modifiers & SHIFT != 0)
        + 2 * u8::from(modifiers & ALT != 0)
        + 4 * u8::from(modifiers & CONTROL != 0)
        + 8 * u8::from(modifiers & (META | SUPER) != 0)
}

fn repeats(count: u16) -> usize {
    usize::from(count.clamp(1, MAX_REPEAT))
}

#[cfg(test)]
mod tests;
