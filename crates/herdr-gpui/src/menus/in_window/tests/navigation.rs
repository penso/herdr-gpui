use super::*;

#[gpui::test]
fn shared_action_dispatches_after_dismissal(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_application_menu(Some(0), window, cx);
        })
    });
    draw(cx);
    let about = cx.debug_bounds("application-item-About Herdr").unwrap();
    cx.simulate_click(about.center(), Modifiers::none());
    cx.run_until_parked();
    draw(cx);
    assert_eq!(
        view.read_with(cx, |view, _| view.menu.page),
        Some(Page::About)
    );
}

#[gpui::test]
fn arrows_skip_separators_and_disabled_actions(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.focus.focus(window, cx);
            view.open_application_menu(Some(2), window, cx);
        })
    });
    draw(cx);
    view.read_with(cx, |view, _| {
        let menu = view.menu.application.as_ref().unwrap();
        assert_eq!(menu.selectable(), vec![2]); // Only Paste acts on an unselected terminal.
        assert_eq!(menu.selected, Some(2));
    });
    cx.simulate_keystrokes("down up");
    assert_eq!(
        view.read_with(cx, |view, _| view
            .menu
            .application
            .as_ref()
            .unwrap()
            .selected),
        Some(2)
    );
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| {
        assert!(view.read(cx).menu.page.is_none());
        assert!(view.read(cx).focus.is_focused(window));
    });
}

#[gpui::test]
fn nested_layout_menu_preserves_checked_state_and_back_navigation(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_application_menu(Some(3), window, cx);
        })
    });
    draw(cx);
    let layout = cx.debug_bounds("application-item-Layout").unwrap();
    cx.simulate_click(layout.center(), Modifiers::none());
    draw(cx);
    view.read_with(cx, |view, _| {
        let menu = view.menu.application.as_ref().unwrap();
        assert_eq!(menu.current().unwrap().name.as_ref(), "Layout");
        let checked: Vec<_> = menu
            .current()
            .unwrap()
            .items
            .iter()
            .filter_map(|item| match item {
                OwnedMenuItem::Action {
                    action,
                    checked: true,
                    ..
                } => Some(
                    action
                        .as_any()
                        .downcast_ref::<crate::actions::SetLayout>()
                        .unwrap()
                        .mode,
                ),
                _ => None,
            })
            .collect();
        assert_eq!(checked, vec![view.config.layout.mode]);
    });
    cx.simulate_keystrokes("left");
    assert_eq!(
        view.read_with(cx, |view, _| view
            .menu
            .application
            .as_ref()
            .unwrap()
            .path
            .clone()),
        vec![3]
    );
    cx.simulate_keystrokes("left");
    assert_eq!(
        view.read_with(cx, |view, _| view
            .menu
            .application
            .as_ref()
            .unwrap()
            .path
            .clone()),
        vec![2]
    );
}
