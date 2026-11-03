//! The `en-XA` leak test (`crate::i18n::leak_tests`) for Keyboard Shortcuts
//! and its shortcut editor: each draws only translated messages, except
//! for the keycaps of the shortcuts, which name keys as the keymap does.
use super::*;
use crate::i18n::leak_tests::{assert_translated, draw, under_en_xa};

/// Every keycap the page can draw: each command's shortcuts, the paste row's,
/// and the palette's double Shift.
fn keycaps(view: &HerdrWindow) -> Vec<String> {
    let keymap = view.keymap();
    COMMANDS
        .iter()
        .flat_map(|info| keymap.shortcuts(info.command))
        .chain(crate::actions::EDIT_KEYS)
        .chain(["shift shift"])
        .flat_map(crate::menu::settings::keycaps)
        .collect()
}

#[test]
fn keyboard_shortcuts_and_the_editor_show_only_messages_under_en_xa() {
    under_en_xa("keyboard shortcuts en-XA leak", |cx, recorder| {
        let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        cx.simulate_resize(size(px(1000.), px(4000.)));
        draw(cx);
        recorder.take();
        view.update_in(cx, |view, window, cx| view.open_keybinds(window, cx));
        draw(cx);
        let data = view.read_with(cx, |view, _| keycaps(view));
        assert_translated("Keyboard Shortcuts", &recorder.take(), &data);

        view.update_in(cx, |view, window, cx| {
            view.edit_shortcut(info(Command::ReloadConfig), window, cx)
        });
        draw(cx);
        assert_translated("Keyboard Shortcuts > Edit", &recorder.take(), &data);
    });
}
