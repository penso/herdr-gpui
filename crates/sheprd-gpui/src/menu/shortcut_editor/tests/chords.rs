use super::*;
use crate::controls::platform_default;
use std::sync::mpsc;

/// The field lists a command's chords beside its keystrokes, so removing one
/// word frees that shortcut alone.
#[gpui::test]
fn removing_the_keystroke_from_the_field_keeps_the_chord(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    let (saved, received) = mpsc::channel();
    view.update_in(cx, |view, window, cx| {
        view.open_keybinds(window, cx);
        view.edit_shortcut(info(Command::Tab), window, cx);
        let input = view.menu.shortcut_editor.as_ref().unwrap().input.clone();
        assert_eq!(
            input.read(cx).text(),
            format!("{} prefix-c", platform_default("cmd-t"))
        );
        input.update(cx, |input, cx| input.set_text_selected("prefix-c", cx));
        view.save_shortcut_with(
            Edit::Save,
            move |name, binding| {
                saved.send((name, binding)).unwrap();
                Ok(())
            },
            window,
            cx,
        );
    });
    cx.run_until_parked();
    let (name, binding) = received.try_recv().unwrap();
    assert_eq!(name, "new_tab");
    let binding = binding.unwrap();
    assert_eq!(binding, Binding::Many(vec!["prefix-c".into()]));
    let keymap = Keymap::with_overrides(
        &[(name.to_owned(), binding)].into_iter().collect(),
        &Default::default(),
        &DaemonKeys::default(),
    )
    .unwrap();
    assert_eq!(
        keymap.shortcuts(Command::Tab).collect::<Vec<_>>(),
        ["ctrl-b c"]
    );
}

/// Unbind still removes every shortcut the command has.
#[gpui::test]
fn unbind_removes_the_chord_too(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    let (saved, received) = mpsc::channel();
    view.update_in(cx, |view, window, cx| {
        view.open_keybinds(window, cx);
        view.edit_shortcut(info(Command::Tab), window, cx);
        view.save_shortcut_with(
            Edit::Unbind,
            move |_, binding| {
                saved.send(binding).unwrap();
                Ok(())
            },
            window,
            cx,
        );
    });
    cx.run_until_parked();
    assert_eq!(
        received.try_recv().unwrap(),
        Some(Binding::Many(Vec::new()))
    );
}
