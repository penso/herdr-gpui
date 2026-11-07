use super::*;

#[gpui::test]
fn turning_the_repository_icons_off_gives_the_name_their_room(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let icon = cx.debug_bounds("github-herdr").unwrap();
    let shown = cx.debug_bounds("name-herdr").unwrap();
    assert!(cx.debug_bounds("github-agent-launcher").is_some());

    view.update(cx, |view, cx| {
        view.config.show_repository_icons = false;
        cx.notify();
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("github-herdr").is_none());
    assert!(cx.debug_bounds("github-agent-launcher").is_none());
    // The name starts where the icon did and keeps its right edge.
    let hidden = cx.debug_bounds("name-herdr").unwrap();
    assert_eq!(hidden.left(), icon.left());
    assert_eq!(hidden.right(), shown.right());
    assert_eq!(
        hidden.size.width,
        shown.size.width + px(crate::sidebar::ICON_RESERVE)
    );

    view.update(cx, |view, cx| {
        view.config.show_repository_icons = true;
        cx.notify();
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert_eq!(cx.debug_bounds("github-herdr"), Some(icon));
    assert_eq!(cx.debug_bounds("name-herdr"), Some(shown));
}

/// Superset draws an icon slot of its own, which the setting leaves in place:
/// only what is drawn inside it changes.
#[gpui::test]
fn superset_keeps_its_icon_slot_without_the_repository_icons(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(800.), px(600.)));
    view.update(cx, |view, cx| {
        view.config.layout.mode = crate::config::LayoutMode::Superset;
        cx.notify();
    });
    cx.run_until_parked();
    let slot = cx.debug_bounds("icon-herdr").unwrap();
    let row = cx.debug_bounds("row-herdr").unwrap();

    view.update(cx, |view, cx| {
        view.config.show_repository_icons = false;
        cx.notify();
    });
    cx.run_until_parked();
    assert_eq!(cx.debug_bounds("icon-herdr"), Some(slot));
    assert_eq!(cx.debug_bounds("row-herdr"), Some(row));
}

/// Configured rows place the icon themselves, beside their first line.
#[gpui::test]
fn configured_rows_give_the_icon_room_to_their_first_line(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(800.), px(600.)));
    view.update(cx, |view, cx| {
        view.config.sidebar_layout.spaces =
            toml::from_str(r#"rows = [["workspace", "branch"]]"#).unwrap();
        cx.notify();
    });
    cx.run_until_parked();
    // One configured line, so the built-in branch line is gone.
    assert!(cx.debug_bounds("detail-herdr").is_none());
    let icon = cx.debug_bounds("github-herdr").unwrap();
    let shown = cx.debug_bounds("name-herdr").unwrap();
    assert_eq!(shown.left(), icon.left() + px(crate::sidebar::ICON_RESERVE));

    view.update(cx, |view, cx| {
        view.config.show_repository_icons = false;
        cx.notify();
    });
    cx.run_until_parked();
    assert!(cx.debug_bounds("github-herdr").is_none());
    let hidden = cx.debug_bounds("name-herdr").unwrap();
    assert_eq!(hidden.left(), icon.left());
    assert_eq!(hidden.right(), shown.right());
}
