//! Mouse reports, and wheel turns as arrow keys under alternate scroll.

use super::{ALT, CONTROL, SHIFT, repeats};
use crate::{Modes, MouseEncoding, MouseReporting};
use herdr_protocol::{ClientMouseButton, ClientMouseKind, ClientMousePosition};

/// The largest 0-based position X10 encoding can carry in one byte.
const X10_MAX: u16 = 255 - 32 - 1;
/// The largest 0-based position UTF-8 encoding can carry in two bytes.
const UTF8_MAX: u16 = 0x7ff - 32 - 1;

pub(super) fn encode(
    kind: ClientMouseKind,
    position: ClientMousePosition,
    modifiers: u8,
    lines: u16,
    modes: &Modes,
) -> Vec<u8> {
    let wanted = match (modes.mouse, kind) {
        (MouseReporting::Off, _) => return alternate_scroll(kind, lines, modes),
        (_, ClientMouseKind::Down(_) | ClientMouseKind::Up(_)) => true,
        (
            _,
            ClientMouseKind::ScrollUp
            | ClientMouseKind::ScrollDown
            | ClientMouseKind::ScrollLeft
            | ClientMouseKind::ScrollRight,
        ) => true,
        (MouseReporting::Click, _) => false,
        (MouseReporting::Drag, ClientMouseKind::Drag(_)) => true,
        (MouseReporting::Drag, _) => false,
        (MouseReporting::Motion, _) => true,
    };
    if !wanted {
        return Vec::new();
    }
    let (column, row) = match position {
        ClientMousePosition::Cell { column, row }
        | ClientMousePosition::Pixels { column, row, .. } => (column, row),
    };
    let held = |button| match button {
        ClientMouseButton::Left => 0,
        ClientMouseButton::Middle => 1,
        ClientMouseButton::Right => 2,
    };
    let (button, scroll) = match kind {
        ClientMouseKind::Down(button) | ClientMouseKind::Up(button) => (held(button), false),
        ClientMouseKind::Drag(button) => (held(button) + 32, false),
        ClientMouseKind::Moved => (3 + 32, false),
        ClientMouseKind::ScrollUp => (64, true),
        ClientMouseKind::ScrollDown => (65, true),
        ClientMouseKind::ScrollLeft => (66, true),
        ClientMouseKind::ScrollRight => (67, true),
    };
    let modifier_bits = 4 * u8::from(modifiers & SHIFT != 0)
        + 8 * u8::from(modifiers & ALT != 0)
        + 16 * u8::from(modifiers & CONTROL != 0);
    let release = matches!(kind, ClientMouseKind::Up(_));
    let report = match modes.mouse_encoding {
        MouseEncoding::Sgr => format!(
            "\x1b[<{};{};{}{}",
            button + modifier_bits,
            u32::from(column) + 1,
            u32::from(row) + 1,
            if release { 'm' } else { 'M' }
        )
        .into_bytes(),
        MouseEncoding::X10 | MouseEncoding::Utf8 => {
            let button = if release { 3 } else { button } + modifier_bits;
            let Some(report) = legacy(button, column, row, modes.mouse_encoding) else {
                return Vec::new();
            };
            report
        }
    };
    report.repeat(if scroll { repeats(lines) } else { 1 })
}

/// `CSI M b x y`, or `None` when the position is past what it can carry.
fn legacy(button: u8, column: u16, row: u16, encoding: MouseEncoding) -> Option<Vec<u8>> {
    let mut out = b"\x1b[M".to_vec();
    out.push(32 + button);
    for value in [column, row] {
        match encoding {
            MouseEncoding::Utf8 if value <= UTF8_MAX => {
                let c = char::from_u32(u32::from(value) + 33)?;
                out.extend_from_slice(c.encode_utf8(&mut [0; 4]).as_bytes());
            }
            MouseEncoding::X10 if value <= X10_MAX => out.push(u8::try_from(value + 33).ok()?),
            _ => return None,
        }
    }
    Some(out)
}

/// With reporting off, the wheel on the alternate screen sends arrow keys
/// when the program left alternate scroll (1007) on, so pagers scroll.
fn alternate_scroll(kind: ClientMouseKind, lines: u16, modes: &Modes) -> Vec<u8> {
    if !modes.alt_screen || !modes.alternate_scroll {
        return Vec::new();
    }
    let letter = match kind {
        ClientMouseKind::ScrollUp => b'A',
        ClientMouseKind::ScrollDown => b'B',
        _ => return Vec::new(),
    };
    let intro = if modes.app_cursor { b'O' } else { b'[' };
    [0x1b, intro, letter].repeat(repeats(lines))
}
