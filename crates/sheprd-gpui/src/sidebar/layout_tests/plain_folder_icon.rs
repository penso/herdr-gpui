use super::*;

/// A space whose folder is not a Git checkout has no branch, so it gets no
/// GitHub mark, but it keeps the icon's room so its name still lines up with
/// the repositories listed around it.
#[gpui::test]
fn a_folder_outside_git_keeps_the_icon_room_without_the_mark(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("github-herdr").is_some());
    let shown = cx.debug_bounds("name-herdr").unwrap();

    view.update(cx, |view, cx| {
        let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        snapshot.workspaces[0].branch = None;
        cx.notify();
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("github-herdr").is_none());
    assert_eq!(cx.debug_bounds("name-herdr"), Some(shown));
    assert!(
        cx.debug_bounds("github-agent-launcher").is_some(),
        "repositories keep their mark"
    );
}
