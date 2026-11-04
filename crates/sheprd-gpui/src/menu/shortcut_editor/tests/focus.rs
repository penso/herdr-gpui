use super::*;
use gpui::{EntityInputHandler, Keystroke, PlatformInput, VisualTestContext, point};

/// Keep a deliberately invalid draft so a regression cannot write real config.
/// It must not even be validated while the search field owns the keyboard.
fn search_with_draft(cx: &mut TestAppContext) -> (Entity<SheprdWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.simulate_resize(size(px(700.), px(600.)));
    view.update_in(cx, |view, window, cx| {
        view.open_keybinds(window, cx);
        view.edit_shortcut(info(Command::Tab), window, cx);
        view.menu
            .shortcut_editor
            .as_ref()
            .unwrap()
            .input
            .update(cx, |input, cx| {
                input.set_text_selected("t", cx);
            });
    });
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
    let search = cx.debug_bounds("keybinds-search-area").unwrap();
    cx.simulate_click(
        point(search.center().x, search.top() + px(20.)),
        Modifiers::default(),
    );
    view.read_with(cx, |view, cx| {
        assert!(view.menu.shortcut_editor.as_ref().unwrap().error.is_none());
        assert_eq!(
            view.menu
                .shortcut_editor
                .as_ref()
                .unwrap()
                .input
                .read(cx)
                .text(),
            "t"
        );
    });
    cx.update(|window, cx| {
        assert!(
            view.read(cx)
                .menu
                .keybinds_search
                .as_ref()
                .unwrap()
                .read(cx)
                .focus
                .is_focused(window)
        );
    });
    (view, cx)
}

#[gpui::test]
fn search_enter_leaves_a_draft_alone_and_refocusing_restores_editor_keys(cx: &mut TestAppContext) {
    let (view, cx) = search_with_draft(cx);
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, cx| {
        let editor = view.menu.shortcut_editor.as_ref().unwrap();
        assert!(
            editor.error.is_none(),
            "search Enter validated the shortcut draft"
        );
        assert!(!editor.saving);
        assert_eq!(editor.input.read(cx).text(), "t");
    });

    view.update_in(cx, |view, window, cx| {
        let input = view.menu.shortcut_editor.as_ref().unwrap().input.clone();
        window.focus(&input.read(cx).focus.clone(), cx);
    });
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        assert!(view.menu.shortcut_editor.as_ref().unwrap().error.is_some());
    });
    cx.simulate_keystrokes("escape");
    view.read_with(cx, |view, _| {
        assert!(view.menu.shortcut_editor.is_none());
        assert_eq!(view.menu.page, Some(Page::Keybinds));
    });
}

#[gpui::test]
fn search_composition_enter_and_escape_cannot_save_or_discard_a_draft(cx: &mut TestAppContext) {
    let (view, cx) = search_with_draft(cx);
    let search = view.read_with(cx, |view, _| {
        view.menu.keybinds_search.as_ref().unwrap().clone()
    });
    cx.update(|window, cx| {
        search.update(cx, |input, cx| {
            input.replace_and_mark_text_in_range(None, "日", Some(1..1), window, cx);
        });
        window.draw(cx).clear(cx);
        // Raw dispatch leaves the IME state intact; simulate_keystrokes would
        // commit composition before the focused element sees these events.
        for key in ["enter", "escape"] {
            window.dispatch_event(
                PlatformInput::KeyDown(KeyDownEvent {
                    keystroke: Keystroke::parse(key).unwrap(),
                    is_held: false,
                    prefer_character_input: false,
                }),
                cx,
            );
        }
    });
    view.read_with(cx, |view, cx| {
        let editor = view.menu.shortcut_editor.as_ref().unwrap();
        assert_eq!(view.menu.page, Some(Page::Keybinds));
        assert_eq!(editor.input.read(cx).text(), "t");
        assert!(editor.error.is_none());
        assert!(!editor.saving);
        assert!(search.read(cx).is_composing());
    });
}
