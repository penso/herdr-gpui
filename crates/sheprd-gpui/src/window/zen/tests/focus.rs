use super::*;
use crate::editor::EditorTarget;

/// A code tab holds the keyboard with its own handle, so toggling zen mode
/// leaves the keyboard with it rather than with the hidden terminal.
#[gpui::test]
fn a_code_tab_keeps_the_keyboard_through_zen_mode(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("main.rs");
    std::fs::write(&path, "fn main() {}\n").unwrap();
    let (view, cx) = window(cx);
    let target = EditorTarget {
        path,
        line: Some(1),
    };
    cx.update(|window, cx| view.update(cx, |view, cx| view.open_code_view(&target, window, cx)));
    cx.run_until_parked();
    draw(cx);
    let focused = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            let view = view.read(cx);
            let code = view.code_views.values().next().unwrap();
            (code.focus.is_focused(window), view.focus.is_focused(window))
        })
    };
    assert_eq!(focused(cx), (true, false));

    run(&view, Command::ToggleZenMode, cx);
    assert_eq!(focused(cx), (true, false));
    run(&view, Command::ToggleZenMode, cx);
    assert_eq!(focused(cx), (true, false));
}

/// With the code tab in use but the keyboard left with the terminal in
/// another group, zen mode hides that terminal, so the tab takes it.
#[gpui::test]
fn zen_hands_the_keyboard_to_the_tab_in_use(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("main.rs");
    std::fs::write(&path, "fn main() {}\n").unwrap();
    let (view, cx) = window(cx);
    let target = EditorTarget {
        path,
        line: Some(1),
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_code_view(&target, window, cx);
            window.focus(&view.focus, cx);
        })
    });
    cx.run_until_parked();
    draw(cx);

    run(&view, Command::ToggleZenMode, cx);
    let in_tab = cx.update(|window, cx| {
        let view = view.read(cx);
        view.code_views
            .values()
            .next()
            .unwrap()
            .focus
            .is_focused(window)
    });
    assert!(in_tab);
}

/// Something zen mode hides, such as the sidebar's search, gives the
/// keyboard back to the terminal.
#[gpui::test]
fn zen_takes_the_keyboard_from_the_hidden_sidebar(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        let focus = view.read(cx).sidebar_search.focus(cx);
        window.focus(&focus, cx);
    });
    draw(cx);

    run(&view, Command::ToggleZenMode, cx);
    let terminal = cx.update(|window, cx| view.read(cx).focus.is_focused(window));
    assert!(terminal);
}

/// The sidebar can still be used while it slides off, so its search may
/// take the keyboard after the toggle; once the slide ends and hides it,
/// the keyboard goes back to the terminal.
#[gpui::test]
fn the_search_focused_mid_slide_gives_the_keyboard_back(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.zen.hold_slides();
            view.command(Command::ToggleZenMode, window, cx);
            let search = view.sidebar_search.focus(cx);
            window.focus(&search, cx);
        })
    });
    draw(cx);
    let terminal =
        |cx: &mut VisualTestContext| cx.update(|window, cx| view.read(cx).focus.is_focused(window));
    assert!(!terminal(cx));

    view.update(cx, |view, _| view.zen.finish_slide());
    draw(cx);
    assert!(view.read_with(cx, |view, _| view.zen_layout()));
    assert!(terminal(cx));
}

/// A menu opened while the chrome slides off keeps the keyboard when the
/// slide ends.
#[gpui::test]
fn a_menu_opened_mid_slide_keeps_the_keyboard(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.zen.hold_slides();
            view.command(Command::ToggleZenMode, window, cx);
            view.command(Command::Palette, window, cx);
        })
    });
    draw(cx);
    let in_menu = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| {
            let view = view.read(cx);
            !view.focus.is_focused(window) && view.menu.page.is_some()
        })
    };
    assert!(in_menu(cx));

    view.update(cx, |view, _| view.zen.finish_slide());
    draw(cx);
    assert!(view.read_with(cx, |view, _| view.zen_layout()));
    assert!(in_menu(cx));
}
