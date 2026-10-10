use super::*;
use crate::{menu::Page, sidebar::layout_tests::full_draw};

fn open_edit_menu(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_application_menu(Some(2), window, cx)
        });
        full_draw(window, cx).clear(cx);
        assert!(view.read(cx).menu.focus.is_focused(window));
        assert!(!view.read(cx).find_focused(window, cx));
    });
}

#[gpui::test]
fn dismissing_application_menus_restores_the_find_field(cx: &mut TestAppContext) {
    let (view, _peer, cx) = open(cx, &["pane.copy_search"]);
    find(&view, cx);
    let focus = cx.update(|window, cx| window.focused(cx).unwrap());
    for escape in [true, false] {
        open_edit_menu(&view, cx);
        cx.simulate_keystrokes("a b c");
        if escape {
            cx.simulate_keystrokes("escape");
        } else {
            let outside = cx.update(|window, _| {
                let viewport = window.viewport_size();
                point(viewport.width - px(8.), viewport.height - px(8.))
            });
            cx.simulate_click(outside, Modifiers::none());
        }
        cx.update(|window, cx| {
            full_draw(window, cx).clear(cx);
            let view = view.read(cx);
            assert!(view.menu.page.is_none());
            assert!(focus.is_focused(window));
            assert!(view.find_focused(window, cx));
            assert!(!view.focus.is_focused(window));
            assert_eq!(view.find.as_ref().unwrap().input.read(cx).text(), "");
        });
    }
}

#[gpui::test]
fn edit_menu_paste_targets_the_restored_find_field_not_the_terminal(cx: &mut TestAppContext) {
    let (view, mut peer, cx) = open(cx, &["pane.copy_search"]);
    find(&view, cx);
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("needle".into())));
    open_edit_menu(&view, cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.menu.page),
        Some(Page::Application)
    );
    let paste = cx.debug_bounds("application-item-Paste").unwrap();
    cx.simulate_click(paste.center(), Modifiers::none());
    cx.run_until_parked();
    cx.update(|window, cx| {
        full_draw(window, cx).clear(cx);
        let view = view.read(cx);
        assert!(view.menu.page.is_none());
        assert!(view.find_focused(window, cx));
        assert!(!view.focus.is_focused(window));
        assert_eq!(view.find.as_ref().unwrap().input.read(cx).text(), "needle");
    });
    // This helper rejects terminal input on the wire before reading the query.
    let request = next_request(&mut peer);
    assert_eq!(request["method"], "pane.copy_search");
    assert_eq!(request["params"]["query"], "needle");
}
