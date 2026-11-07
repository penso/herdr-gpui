use super::*;

#[gpui::test]
fn a_long_workspace_name_wraps_in_the_popover_header(cx: &mut gpui::TestAppContext) {
    let label = "#689 feat: open px0 in workspace tabs with shared checkout and a long name";
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        let mut view = fixture_window(window, cx);
        view.live.status = crate::state::ConnectionStatus::Connected;
        Arc::make_mut(view.live.snapshot.as_mut().unwrap()).workspaces[0].label = label.into();
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_workspace_menu("w0", point(px(20.), px(100.)), window, cx)
        });
        full_draw(window, cx).clear(cx);
    });
    let panel = cx.debug_bounds("menu-panel").unwrap();
    let name = cx.debug_bounds("workspace-menu-name").unwrap();
    let branch = cx.debug_bounds("workspace-menu-branch").unwrap();
    assert!(name.left() >= panel.left() && name.right() <= panel.right());
    // The whole name is laid out over several lines rather than cut short.
    assert!(
        name.size.height >= branch.size.height * 2.,
        "{name:?} {branch:?}"
    );
    assert!(branch.top() >= name.bottom());
    cx.update(|_, cx| {
        let probe = cx.global::<TextProbes>().0[label].0;
        assert!(probe.size.height >= branch.size.height * 2., "{probe:?}");
    });
}
