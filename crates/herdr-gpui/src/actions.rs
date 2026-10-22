//! GPUI actions the app registers and the keystrokes bound to them. Every
//! shortcut comes from the config's resolved `Keymap`, so the palette, the
//! menu bar, and the keymap cannot drift apart. The macOS Hide and Minimize
//! items are the exception: they are not catalog commands, and keep the
//! keystrokes macOS gives every app unless the keymap binds a command to one.

use crate::controls::{CTRL_SHORTCUTS, Command};
use gpui::{Action, App, KeyBinding, KeyDownEvent, Keystroke, Modifiers, actions};

actions!(
    herdr,
    [
        Quit,
        Hide,
        HideOthers,
        ShowAll,
        Minimize,
        ZoomWindow,
        PlaySound,
        RingBellPreview,
        ShowHerdrNotDetected,
        ShowLogs,
        CheckForUpdates,
        ShowUpdatePreview,
        ShowUpdateDownloadPreview,
        ShowUpdateHomebrewPreview
    ]
);

// Edit menu items. Each element registers only the ones it can perform, so
// GPUI disables the rest while that element has focus.
actions!(edit, [Cut, Copy, Paste, SelectAll]);

/// No element sets this context, so these bindings never match a keystroke.
/// They exist only so the Edit menu shows its standard shortcuts. The focused
/// element's key handler keeps owning Cmd-X/C/V/A, as it did before the menu.
const EDIT_MENU_LABELS: &str = "EditMenuLabels";

/// The keystroke an Edit menu item stands for. A menu click replays it
/// through the focused element's own key handler, so choosing the item and
/// pressing its shortcut cannot behave differently.
pub(crate) fn edit_key(key: &str) -> KeyDownEvent {
    KeyDownEvent {
        keystroke: Keystroke {
            modifiers: Modifiers {
                platform: !CTRL_SHORTCUTS,
                control: CTRL_SHORTCUTS,
                shift: CTRL_SHORTCUTS,
                ..Modifiers::default()
            },
            key: key.into(),
            key_char: None,
        },
        is_held: false,
        prefer_character_input: false,
    }
}

#[derive(Clone, PartialEq, serde::Deserialize, Action)]
#[action(no_json)]
pub(crate) struct RunCommand {
    pub(crate) command: Command,
}

/// Picks the sidebar layout, from View > Layout.
#[derive(Clone, PartialEq, serde::Deserialize, Action)]
#[action(no_json)]
pub(crate) struct SetLayout {
    pub(crate) mode: crate::config::LayoutMode,
}

/// Picks how tab strips draw their tabs, from View > Tab Layout.
#[derive(Clone, PartialEq, serde::Deserialize, Action)]
#[action(no_json)]
pub(crate) struct SetTabLayout {
    pub(crate) mode: crate::config::TabLayoutMode,
}

#[derive(Clone, PartialEq, serde::Deserialize, Action)]
#[action(no_json)]
pub(crate) struct ShowToastPreview {
    pub(crate) kind: herdr_client::protocol::SemanticNotificationKind,
}

/// QA: posts an OS notification for the focused pane after a short delay.
#[derive(Clone, PartialEq, serde::Deserialize, Action)]
#[action(no_json)]
pub(crate) struct ShowSystemNotificationPreview;

#[cfg(any(target_os = "macos", windows, test))]
#[derive(Clone, PartialEq, serde::Deserialize, Action)]
#[action(no_json)]
pub(crate) struct SetBadgePreview {
    pub(crate) enabled: bool,
}

/// Binds the active window's server keymap when its device opted into one,
/// otherwise the keymap from the last validated config, or the catalog
/// defaults before any config has loaded.
pub(crate) fn bind_keys(cx: &mut App) {
    let keymap = cx
        .try_global::<crate::window::ActiveServerKeymap>()
        .and_then(|active| active.0.clone())
        .or_else(|| {
            cx.try_global::<crate::app::InitialAppearance>()
                .map(|appearance| appearance.config.keybindings.clone())
        })
        .unwrap_or_default();
    #[cfg(target_os = "macos")]
    crate::browser::bind_page_keys(&keymap);
    cx.bind_keys(keymap.bindings().map(|(command, keystroke)| {
        if command == Command::Quit {
            KeyBinding::new(keystroke, Quit, None)
        } else {
            KeyBinding::new(keystroke, RunCommand { command }, None)
        }
    }));
    cx.bind_keys(crate::log_window::key_bindings());
    cx.bind_keys(crate::settings_window::key_bindings());
    let [cut, copy, paste, select_all] = EDIT_KEYS;
    cx.bind_keys([
        KeyBinding::new(cut, Cut, Some(EDIT_MENU_LABELS)),
        KeyBinding::new(copy, Copy, Some(EDIT_MENU_LABELS)),
        KeyBinding::new(paste, Paste, Some(EDIT_MENU_LABELS)),
        KeyBinding::new(select_all, SelectAll, Some(EDIT_MENU_LABELS)),
    ]);
    // Bound last, where they outrank bindings at the same context depth. One
    // the keymap binds a command to is left out instead, so its menu item
    // shows no shortcut it does not have.
    #[cfg(target_os = "macos")]
    {
        let [hide, hide_others, minimize] = SYSTEM_KEYS;
        let free: Vec<_> = free_system_keys(&keymap).collect();
        cx.bind_keys(
            [
                KeyBinding::new(hide, Hide, None),
                KeyBinding::new(hide_others, HideOthers, None),
                KeyBinding::new(minimize, Minimize, None),
            ]
            .into_iter()
            .zip(SYSTEM_KEYS)
            .filter(|(_, keystroke)| free.contains(keystroke))
            .map(|(binding, _)| binding),
        );
    }
}

/// The Edit menu's keystrokes: cut, copy, paste, and select all.
pub(crate) const EDIT_KEYS: [&str; 4] = if CTRL_SHORTCUTS {
    [
        "ctrl-shift-x",
        "ctrl-shift-c",
        "ctrl-shift-v",
        "ctrl-shift-a",
    ]
} else {
    ["cmd-x", "cmd-c", "cmd-v", "cmd-a"]
};

/// Fields also accept plain Ctrl on Linux and Windows, where there is no
/// terminal program to receive it. Keep the terminal's Ctrl-Shift aliases
/// working in fields so Edit menu clicks and keyboard shortcuts share the
/// same path.
pub(crate) fn edit_modifiers(modifiers: Modifiers) -> bool {
    !modifiers.alt
        && !modifiers.function
        && ((modifiers.platform && !modifiers.control && !modifiers.shift)
            || (CTRL_SHORTCUTS && modifiers.control && !modifiers.platform))
}

/// Whether a field moves or deletes by word on an arrow, Backspace, or
/// Delete: Option on macOS, Ctrl on the other desktops. Shift extends.
pub(crate) fn word_modifiers(modifiers: Modifiers) -> bool {
    let (word, other) = if cfg!(target_os = "macos") {
        (modifiers.alt, modifiers.control)
    } else {
        (modifiers.control, modifiers.alt)
    };
    word && !other && !modifiers.platform && !modifiers.function
}

/// macOS's Hide, Hide Others, and Minimize, bound there alone.
const SYSTEM_KEYS: [&str; 3] = ["cmd-h", "cmd-alt-h", "cmd-m"];

/// The system keystrokes the app keeps: each one `keymap` binds no command
/// to. No default is bound to any of them, so a command that holds one was
/// put there on purpose, in `[keybindings]` or the daemon's `[keys]`, and
/// runs instead.
#[cfg(any(target_os = "macos", test))]
fn free_system_keys(keymap: &crate::keymap::Keymap) -> impl Iterator<Item = &'static str> {
    let bound: Vec<Keystroke> = keymap
        .bindings()
        .filter_map(|(_, label)| Keystroke::parse(label).ok())
        .collect();
    SYSTEM_KEYS.into_iter().filter(move |system| {
        Keystroke::parse(system).is_ok_and(|system| {
            bound
                .iter()
                .all(|bound| (bound.modifiers, &bound.key) != (system.modifiers, &system.key))
        })
    })
}

/// Keystrokes a daemon custom command never takes: the Edit keys and the
/// macOS system keys. Its direct keystroke is matched before any binding, so
/// one of these would otherwise take Paste or Hide away from the app. A
/// system key the keymap gave to a command is held there instead, so a custom
/// command gets it neither way.
pub(crate) fn reserved_keystrokes() -> impl Iterator<Item = &'static str> {
    let system: &[&str] = if cfg!(target_os = "macos") {
        &SYSTEM_KEYS
    } else {
        &[]
    };
    EDIT_KEYS.into_iter().chain(system.iter().copied())
}

/// Replaces every binding after a config reload. The menu bar reads its
/// shortcut labels from the keymap when it is installed, so it is rebuilt too.
pub(crate) fn rebind_keys(cx: &mut App) {
    cx.clear_key_bindings();
    bind_keys(cx);
    crate::menus::install(cx);
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
