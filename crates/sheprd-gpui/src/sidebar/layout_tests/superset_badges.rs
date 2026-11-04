use super::*;

/// A corner badge must leave the center of the repository/agent icon visible,
/// not just fit inside the slot, a working agent's turning orbit included.
#[gpui::test]
fn superset_badges_leave_the_main_icon_center_visible(cx: &mut gpui::TestAppContext) {
    use crate::config::LayoutMode;
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(800.), px(900.)));
    for font_size in [12., 20., 36.] {
        view.update(cx, |view, cx| {
            view.config.layout.mode = LayoutMode::Superset;
            view.config.sidebar.size = font_size;
            cx.notify();
        });
        cx.run_until_parked();
        for (key, icon, badge) in [
            ("herdr", "icon-herdr", "dot-herdr"),
            ("agent-p0", "icon-agent-p0", "dot-agent-p0"),
        ] {
            let icon = cx.debug_bounds(icon).unwrap();
            let badge = cx.debug_bounds(badge).unwrap();
            assert!(
                !badge.contains(&icon.center()),
                "dots at {font_size}: {key} badge {badge:?} masks icon center {icon:?}"
            );
            assert!(badge.size.width <= icon.size.width / 2.);
            assert!(badge.size.height <= icon.size.height / 2.);
        }
    }
}
