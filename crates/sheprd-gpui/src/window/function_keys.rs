//! A function key macOS delivers without its name.
//!
//! AppKit names a function key by a private-use character only when the
//! event carries the function flag. A key remapped to one with `hidutil`
//! (Caps Lock to F20, say) arrives without the flag, so GPUI names it by
//! what the keyboard layout types for that key code instead: U+0010 where
//! the layout has an entry, as the U.S. and Norwegian layouts have for F1
//! to F16, and nothing at all where it has none, as for F17 to F20. GPUI
//! also takes U+0010 for a shifted symbol and drops Shift from it. The
//! event's key code and modifier flags still say which key was pressed, and
//! whether with Shift.

use gpui::{Keystroke, Modifiers};

/// What a keyboard layout types for a function key it has an entry for.
const DATA_LINK_ESCAPE: &str = "\u{10}";

/// `keystroke` under its function key's name, when macOS delivered it without
/// one. `press` reads the key code and Shift of the key being handled, which
/// the window takes from [`current_press`]. A keystroke the app dispatches by
/// itself has no event of its own, and is left alone because it carries a
/// name, so `press` is only asked for one that does not.
pub(super) fn named(
    keystroke: &Keystroke,
    press: impl FnOnce() -> Option<(u16, bool)>,
) -> Option<Keystroke> {
    if !unnamed(keystroke) {
        return None;
    }
    let (code, shift) = press()?;
    rename(keystroke, code, shift)
}

fn unnamed(keystroke: &Keystroke) -> bool {
    keystroke.key.is_empty() || keystroke.key == DATA_LINK_ESCAPE
}

/// `shift` comes from the event, not from GPUI, which may have dropped it.
fn rename(keystroke: &Keystroke, code: u16, shift: bool) -> Option<Keystroke> {
    Some(Keystroke {
        modifiers: Modifiers {
            shift,
            ..keystroke.modifiers
        },
        key: name(code)?.to_owned(),
        key_char: None,
    })
}

/// F1 to F20 by macOS virtual key code.
fn name(code: u16) -> Option<&'static str> {
    Some(match code {
        122 => "f1",
        120 => "f2",
        99 => "f3",
        118 => "f4",
        96 => "f5",
        97 => "f6",
        98 => "f7",
        100 => "f8",
        101 => "f9",
        109 => "f10",
        103 => "f11",
        111 => "f12",
        105 => "f13",
        107 => "f14",
        113 => "f15",
        106 => "f16",
        64 => "f17",
        79 => "f18",
        80 => "f19",
        90 => "f20",
        _ => return None,
    })
}

/// The key code of AppKit's current key event, and whether Shift is down.
/// The current event is the last one AppKit took off its queue: the handled
/// keystroke's own while a key handler runs for it, as every caller does.
#[cfg(target_os = "macos")]
pub(super) fn current_press() -> Option<(u16, bool)> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSEventModifierFlags, NSEventType};

    let event = NSApplication::sharedApplication(MainThreadMarker::new()?).currentEvent()?;
    // Only a key event has a key code; AppKit raises for any other.
    let event_type = event.r#type();
    (event_type == NSEventType::KeyDown || event_type == NSEventType::KeyUp).then(|| {
        (
            event.keyCode(),
            event.modifierFlags().contains(NSEventModifierFlags::Shift),
        )
    })
}

#[cfg(not(target_os = "macos"))]
pub(super) fn current_press() -> Option<(u16, bool)> {
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
