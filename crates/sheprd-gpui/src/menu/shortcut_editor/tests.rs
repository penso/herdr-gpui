use super::*;
use crate::controls::{COMMANDS, Command};
use gpui::{Modifiers, TestAppContext, size};

mod en_xa;
mod focus;

fn info(command: Command) -> &'static CommandInfo {
    COMMANDS
        .iter()
        .find(|info| info.command == command)
        .unwrap()
}

#[gpui::test]
fn editor_shows_unbound_commands_and_cancels_without_changing_bindings(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.simulate_resize(size(px(700.), px(600.)));
    view.update_in(cx, |view, window, cx| view.open_keybinds(window, cx));
    view.update(cx, |view, cx| {
        view.menu
            .keybinds_search
            .as_ref()
            .unwrap()
            .update(cx, |search, cx| {
                search.set_text_selected("Reload Config", cx)
            });
    });
    cx.run_until_parked();
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let button = cx.debug_bounds("edit-shortcut-reload_config").unwrap();
    cx.simulate_click(button.center(), Modifiers::default());
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        let editor = view.menu.shortcut_editor.as_ref().unwrap();
        assert_eq!(editor.info.command, Command::ReloadConfig);
        assert_eq!(editor.input.read(cx).text(), "");
    });
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| {
        assert!(view.menu.shortcut_editor.is_none());
        assert_eq!(view.menu.page, Some(Page::Keybinds));
        assert_eq!(view.config.keybindings, Keymap::default());
    });
}

#[gpui::test]
fn invalid_shortcuts_stay_in_the_editor_and_never_start_a_save(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    view.update_in(cx, |view, window, cx| {
        view.open_keybinds(window, cx);
        view.edit_shortcut(info(Command::Tab), window, cx);
        view.menu
            .shortcut_editor
            .as_ref()
            .unwrap()
            .input
            .update(cx, |input, cx| input.set_text_selected("t", cx));
        view.save_shortcut_with(
            Edit::Save,
            |_, _| panic!("invalid shortcut reached disk"),
            window,
            cx,
        );
        let editor = view.menu.shortcut_editor.as_ref().unwrap();
        assert!(editor.error.is_some());
        assert!(!editor.saving);
    });
}

#[gpui::test]
fn save_failure_keeps_the_draft_and_old_keymap(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    view.update_in(cx, |view, window, cx| {
        view.open_keybinds(window, cx);
        view.edit_shortcut(info(Command::Tab), window, cx);
        view.menu
            .shortcut_editor
            .as_ref()
            .unwrap()
            .input
            .update(cx, |input, cx| {
                input.set_text_selected("alt-t ctrl-shift-t", cx)
            });
        view.save_shortcut_with(
            Edit::Save,
            |name, binding| {
                assert_eq!(name, "new_tab");
                assert_eq!(
                    binding,
                    Some(Binding::Many(vec!["alt-t".into(), "ctrl-shift-t".into()]))
                );
                Err(
                    std::io::Error::new(std::io::ErrorKind::PermissionDenied, "read-only config")
                        .into(),
                )
            },
            window,
            cx,
        );
    });
    cx.run_until_parked();
    view.read_with(cx, |view, cx| {
        let editor = view.menu.shortcut_editor.as_ref().unwrap();
        assert!(!editor.saving);
        assert!(editor.error.as_ref().unwrap().contains("read-only config"));
        assert_eq!(editor.input.read(cx).text(), "alt-t ctrl-shift-t");
        assert_eq!(view.config.keybindings, Keymap::default());
    });
}

#[cfg(target_os = "linux")]
#[gpui::test]
fn linux_edit_keys_work_in_the_editor_and_composition_does_not_submit(cx: &mut TestAppContext) {
    use gpui::{ClipboardItem, EntityInputHandler};
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    view.update_in(cx, |view, window, cx| {
        crate::bind_keys(cx);
        view.open_keybinds(window, cx);
        view.edit_shortcut(info(Command::Tab), window, cx);
        cx.write_to_clipboard(ClipboardItem::new_string("ctrl-shift-y".into()));
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    cx.simulate_keystrokes("ctrl-shift-a ctrl-shift-v");
    view.read_with(cx, |view, cx| {
        assert_eq!(
            view.menu
                .shortcut_editor
                .as_ref()
                .unwrap()
                .input
                .read(cx)
                .text(),
            "ctrl-shift-y"
        );
    });
    view.update_in(cx, |view, window, cx| {
        let input = view.menu.shortcut_editor.as_ref().unwrap().input.clone();
        input.update(cx, |input, cx| {
            input.replace_and_mark_text_in_range(None, "日", Some(1..1), window, cx)
        });
        // Exercise the owner's handler with composition still active. The
        // headless platform otherwise commits composition before dispatch.
        for key in ["enter", "escape"] {
            assert!(view.shortcut_editor_key(
                &KeyDownEvent {
                    keystroke: gpui::Keystroke::parse(key).unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                },
                window,
                cx
            ));
        }
    });
    view.read_with(cx, |view, cx| {
        let editor = view.menu.shortcut_editor.as_ref().unwrap();
        assert!(editor.input.read(cx).is_composing());
        assert!(!editor.saving);
        assert!(editor.error.is_none());
    });
}

#[gpui::test]
fn editor_controls_fit_narrow_windows(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    for width in [900., 360.] {
        cx.simulate_resize(size(px(width), px(600.)));
        view.update_in(cx, |view, window, cx| {
            view.open_keybinds(window, cx);
            view.edit_shortcut(info(Command::AnnotateSelection), window, cx);
        });
        cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
        let body = cx.debug_bounds("keybinds-body").unwrap();
        for id in ["shortcut-save", "shortcut-unbind", "shortcut-reset"] {
            let button = cx.debug_bounds(id).unwrap();
            assert!(
                button.left() >= body.left() && button.right() <= body.right(),
                "{id}: {button:?}"
            );
            assert!(
                button.top() >= body.top() && button.bottom() <= body.bottom(),
                "{id}: {button:?}"
            );
        }
    }
}
