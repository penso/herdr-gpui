use super::*;
use crate::controls::{COMMANDS, Command};

/// The menu bar shows the keymap's own spelling for every item, so Windows
/// and Linux read Ctrl text rather than a Win or Super glyph.
#[gpui::test]
fn menu_items_show_ctrl_bindings_without_the_platform_key(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        let view = view.read(cx);
        let platform = ["cmd-", "super-", "win-", "\u{229e}", "\u{2756}"];
        for info in COMMANDS {
            let label = view.application_shortcut(
                &crate::RunCommand {
                    command: info.command,
                },
                cx,
            );
            assert_eq!(label, view.keymap().primary(info.command), "{}", info.name);
            assert!(platform.iter().all(|key| !label.contains(key)), "{label}");
        }
        let quit = view.application_shortcut(&crate::actions::Quit, cx);
        assert_eq!(quit, view.keymap().primary(Command::Quit));
        let [cut, copy, paste, select_all] = crate::actions::EDIT_KEYS;
        for (action, key) in [
            (&crate::actions::Cut as &dyn Action, cut),
            (&crate::actions::Copy, copy),
            (&crate::actions::Paste, paste),
            (&crate::actions::SelectAll, select_all),
        ] {
            assert_eq!(view.application_shortcut(action, cx), key);
        }
    });
}
