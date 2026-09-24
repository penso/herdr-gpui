//! GPUI actions the app registers and the keystrokes bound to them. Every
//! shortcut comes from the config's resolved `Keymap`, so the palette, the
//! menu bar, and the keymap cannot drift apart. The macOS Hide and Minimize
//! items are the exception: macOS reserves those keystrokes for every app,
//! so they stay fixed rather than user-rebindable.

use crate::controls::Command;
use gpui::{Action, App, KeyBinding, KeyDownEvent, Keystroke, Modifiers, actions};

actions!(
    herdr,
    [
        Quit,
        Hide,
        HideOthers,
        ShowAll,
        Minimize,
        PlaySound,
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
                platform: true,
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

#[derive(Clone, PartialEq, serde::Deserialize, Action)]
#[action(no_json)]
pub(crate) struct ShowToastPreview {
    pub(crate) kind: herdr_client::protocol::SemanticNotificationKind,
}

#[cfg(any(target_os = "macos", test))]
#[derive(Clone, PartialEq, serde::Deserialize, Action)]
#[action(no_json)]
pub(crate) struct SetBadgePreview {
    pub(crate) enabled: bool,
}

/// Binds the keymap from the last validated config, or the catalog defaults
/// before any config has loaded.
pub(crate) fn bind_keys(cx: &mut App) {
    let keymap = cx
        .try_global::<crate::app::InitialAppearance>()
        .map(|appearance| appearance.config.keybindings.clone())
        .unwrap_or_default();
    cx.bind_keys(keymap.bindings().map(|(command, keystroke)| {
        if command == Command::Quit {
            KeyBinding::new(keystroke, Quit, None)
        } else {
            KeyBinding::new(keystroke, RunCommand { command }, None)
        }
    }));
    cx.bind_keys(crate::log_window::key_bindings());
    cx.bind_keys([
        KeyBinding::new("cmd-x", Cut, Some(EDIT_MENU_LABELS)),
        KeyBinding::new("cmd-c", Copy, Some(EDIT_MENU_LABELS)),
        KeyBinding::new("cmd-v", Paste, Some(EDIT_MENU_LABELS)),
        KeyBinding::new("cmd-a", SelectAll, Some(EDIT_MENU_LABELS)),
    ]);
    // Bound last so a config keystroke can never steal a reserved macOS
    // shortcut: later bindings take precedence at the same context depth.
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("cmd-alt-h", HideOthers, None),
        KeyBinding::new("cmd-m", Minimize, None),
    ]);
}

/// Replaces every binding after a config reload. The menu bar reads its
/// shortcut labels from the keymap when it is installed, so it is rebuilt too.
pub(crate) fn rebind_keys(cx: &mut App) {
    cx.clear_key_bindings();
    bind_keys(cx);
    crate::menus::install(cx);
}
