use super::*;

#[gpui::test]
fn hovering_a_card_previews_in_main_windows_and_leaving_restores(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let before = source.update(cx, |view, _, _| view.theme.clone()).unwrap();
    let weak = cx.update(|cx| source.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = SettingsWindow::new(weak, cx);
        view.theme_io = Some(recording_themes(Default::default(), false));
        view.initialize_theme_browser(window, cx);
        view
    });
    cx.run_until_parked();
    let index = view.read_with(cx, |view, _| {
        view.themes
            .filtered
            .iter()
            .position(|choice| choice.name == "Nord")
            .unwrap()
    });
    let own = view.read_with(cx, |view, _| view.theme.clone());
    view.update(cx, |view, cx| view.hover_theme(Some(index), cx));
    cx.run_until_parked();
    let hovered = source.update(cx, |view, _, _| view.theme.clone()).unwrap();
    assert_ne!(hovered, before, "main window shows the hovered theme");
    assert_eq!(
        view.read_with(cx, |view, _| view.theme.clone()),
        own,
        "settings keeps its own theme"
    );
    assert!(
        !view.read_with(cx, |view, _| view.theme_dirty()),
        "hover is not a draft"
    );
    view.update(cx, |view, cx| view.hover_theme(None, cx));
    cx.run_until_parked();
    assert_eq!(
        source.update(cx, |view, _, _| view.theme.clone()).unwrap(),
        before
    );
    // Leaving before the theme resolves never paints it afterwards.
    view.update(cx, |view, cx| {
        view.hover_theme(Some(index), cx);
        view.hover_theme(None, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        source.update(cx, |view, _, _| view.theme.clone()).unwrap(),
        before
    );
    // Escape ends the preview with nothing else open.
    view.update(cx, |view, cx| view.hover_theme(Some(index), cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.focus(&view.read(cx).focus.clone(), cx));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!view.read_with(cx, |view, _| view.theme_hovered()));
    assert_eq!(
        source.update(cx, |view, _, _| view.theme.clone()).unwrap(),
        before
    );
    // Closing mid-hover restores too.
    view.update(cx, |view, cx| view.hover_theme(Some(index), cx));
    cx.run_until_parked();
    assert_ne!(
        source.update(cx, |view, _, _| view.theme.clone()).unwrap(),
        before
    );
    assert!(view.update_in(cx, |view, window, cx| view.should_close(window, cx)));
    assert_eq!(
        source.update(cx, |view, _, _| view.theme.clone()).unwrap(),
        before
    );
}
