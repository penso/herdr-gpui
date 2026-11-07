//! A shifted key GPUI delivered without its Shift.
//!
//! GPUI's macOS backend keeps Shift only on the letters a to z. Any other
//! shifted key is named by the character it types and loses Shift: Shift+1
//! arrives as `!`, and Shift+7 as `&` on a U.S. layout or `/` on a Norwegian
//! one, so a chord such as `prefix+shift+1` never matches. The prefix
//! handler tries the keystroke as GPUI named it first, so a chord bound to
//! the symbol itself, such as the default `prefix+?`, still wins. Only when
//! that matches nothing does it try the key with Shift restored: Shift and
//! the key's unshifted character, read from the active keyboard layout the
//! way GPUI reads it.

use gpui::{Keystroke, Modifiers};
use std::borrow::Cow;

/// A keystroke the prefix handler matches: as GPUI named it, then with the
/// Shift GPUI dropped restored, when it dropped one.
pub(in crate::window) struct Typed<'a> {
    keystroke: &'a Keystroke,
    restored: Option<Keystroke>,
}

impl<'a> Typed<'a> {
    pub(in crate::window) fn new(keystroke: &'a Keystroke, restored: Option<Keystroke>) -> Self {
        Self {
            keystroke,
            restored,
        }
    }

    pub(in crate::window) fn keystroke(&self) -> &Keystroke {
        self.keystroke
    }

    /// As `find`, except that what the restored keystroke finds counts only
    /// when `held` says nothing holds the keystroke as GPUI named it. `held`
    /// is asked only then, as answering it can take a while.
    pub(in crate::window) fn find_unheld<T>(
        &self,
        lookup: impl Fn(&Keystroke) -> Option<T>,
        held: impl FnOnce(&Keystroke) -> bool,
    ) -> Option<T> {
        lookup(self.keystroke).or_else(|| {
            let found = self.restored.as_ref().and_then(&lookup)?;
            (!held(self.keystroke)).then_some(found)
        })
    }

    /// What `lookup` finds for the keystroke as typed, or else for it with
    /// Shift restored.
    pub(in crate::window) fn find<T>(&self, lookup: impl Fn(&Keystroke) -> Option<T>) -> Option<T> {
        lookup(self.keystroke).or_else(|| self.restored.as_ref().and_then(lookup))
    }

    pub(in crate::window) fn matches(&self, test: impl Fn(&Keystroke) -> bool) -> bool {
        self.find(|keystroke| test(keystroke).then_some(()))
            .is_some()
    }
}

/// What a key types in the active keyboard layout, as GPUI reads it to name
/// a keystroke.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct KeyChars {
    plain: String,
    shift: String,
    cmd: String,
    cmd_shift: String,
    /// The layout types non-ASCII letters, and ASCII ones under Command, as
    /// Russian and Greek do, so GPUI names every key from its Command layout.
    cmd_layout: bool,
}

impl KeyChars {
    /// The unshifted key behind `shifted`, when GPUI names this key
    /// `shifted` after dropping Shift. This follows GPUI's macOS naming
    /// (`gpui_macos`'s `events.rs`): under Command, or on a layout named
    /// from its Command layout, both names come from the Command layout,
    /// and the shifted one falls back to the plain layout's when Command and
    /// Shift type what Command alone does (U.S. and Norwegian digits).
    fn unshifted(&self, shifted: &str, command: bool) -> Option<&str> {
        let (mut base, mut with_shift) = (self.plain.as_str(), Cow::from(&self.shift));
        if command || self.cmd_layout {
            if self.cmd_shift != self.cmd {
                with_shift = Cow::from(&self.cmd_shift);
            } else if self.cmd.to_ascii_uppercase() != self.cmd {
                // GPUI's Ukrainian rule. It only renames an a to z letter,
                // which keeps its Shift, so it never changes the answer
                // here; it stays so these rules read as GPUI's do.
                with_shift = Cow::from(self.cmd.to_ascii_uppercase());
            }
            base = &self.cmd;
        }
        // GPUI keeps Shift on a to z, so it dropped none from those.
        let letter = base.chars().all(|c| c.is_ascii_lowercase());
        (!letter && with_shift == shifted).then_some(base)
    }
}

#[cfg(test)]
impl KeyChars {
    /// A key that types `plain`, and the rest under Shift, Command, and both.
    pub(crate) fn typing([plain, shift, cmd, cmd_shift]: [&str; 4], cmd_layout: bool) -> Self {
        Self {
            plain: plain.into(),
            shift: shift.into(),
            cmd: cmd.into(),
            cmd_shift: cmd_shift.into(),
            cmd_layout,
        }
    }
}

/// `typed` with the Shift GPUI dropped restored: Shift and the key the
/// layout types without it, so `!` becomes `shift-1`. `press` reads the key
/// code of the event and whether Shift is down, and `layout` what that key
/// types. Neither is asked unless `typed` is a single character without
/// Shift, which is the only form GPUI drops Shift from.
pub(in crate::window) fn restore(
    typed: &Keystroke,
    press: impl FnOnce() -> Option<(u16, bool)>,
    layout: impl FnOnce(u16) -> Option<KeyChars>,
) -> Option<Keystroke> {
    if typed.modifiers.shift || typed.key.chars().count() != 1 {
        return None;
    }
    let (code, shift) = press()?;
    if !shift {
        return None;
    }
    let chars = layout(code)?;
    let key = chars.unshifted(&typed.key, typed.modifiers.platform)?;
    Some(Keystroke {
        modifiers: Modifiers {
            shift: true,
            ..typed.modifiers
        },
        key: key.to_owned(),
        key_char: None,
    })
}

/// What key `code` types in the active layout, from AppKit, which gives the
/// characters GPUI's own layout lookup does. The events are made here rather
/// than taken from the one being handled, so the A key can be asked about as
/// GPUI asks. Call it on the main thread, where the window handles keys:
/// elsewhere AppKit's layout lookup blocks.
#[cfg(target_os = "macos")]
pub(in crate::window) fn current_layout(code: u16) -> Option<KeyChars> {
    use objc2_app_kit::{NSEvent, NSEventModifierFlags as Flags, NSEventType};
    use objc2_core_foundation::CGPoint;
    use objc2_foundation::NSString;

    /// The A key, by which GPUI tells a layout named from its Command layout.
    const A: u16 = 0;
    let empty = NSString::new();
    let event = |code| {
        NSEvent::keyEventWithType_location_modifierFlags_timestamp_windowNumber_context_characters_charactersIgnoringModifiers_isARepeat_keyCode(
            NSEventType::KeyDown,
            CGPoint::new(0., 0.),
            Flags::empty(),
            0.,
            0,
            None,
            &empty,
            &empty,
            false,
            code,
        )
    };
    let typed = |event: &NSEvent, flags| {
        event
            .charactersByApplyingModifiers(flags)
            .map(|chars| chars.to_string())
            .unwrap_or_default()
    };
    let key = event(code)?;
    let a = event(A)?;
    Some(KeyChars {
        plain: typed(&key, Flags::empty()),
        shift: typed(&key, Flags::Shift),
        cmd: typed(&key, Flags::Command),
        cmd_shift: typed(&key, Flags::Command | Flags::Shift),
        cmd_layout: !typed(&a, Flags::empty()).is_ascii() && typed(&a, Flags::Command).is_ascii(),
    })
}

/// Only GPUI's macOS naming is followed; elsewhere nothing is restored.
#[cfg(not(target_os = "macos"))]
pub(in crate::window) fn current_layout(_code: u16) -> Option<KeyChars> {
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
