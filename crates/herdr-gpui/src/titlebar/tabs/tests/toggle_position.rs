use super::*;

#[gpui::test]
fn the_sidebar_toggle_stays_at_the_left_edge_without_traffic_lights(cx: &mut TestAppContext) {
    if cfg!(target_os = "macos") {
        return;
    }
    let (view, cx) = window(cx);
    for width in [1200., 640.] {
        cx.simulate_resize(size(px(width), px(600.)));
        for mode in ["compact", "hidden"] {
            cx.update(|_, cx| {
                view.update(cx, |view, cx| {
                    view.settings.shared = Some(
                        crate::herdr_settings::Settings::parse_text(&format!(
                            "[ui]\nsidebar_collapsed_mode = '{mode}'"
                        ))
                        .unwrap(),
                    );
                    cx.notify();
                });
            });
            draw(cx);
            let expanded = cx.debug_bounds("toggle-sidebar").unwrap();
            for _ in 0..2 {
                cx.simulate_click(expanded.center(), Modifiers::default());
                draw(cx);
                assert!(!view.read_with(cx, |view, _| view.sidebar_visible));
                let collapsed = cx.debug_bounds("toggle-sidebar").unwrap();
                assert_eq!(expanded, collapsed, "{mode}, {width}px");
                if mode == "compact" {
                    let header = cx.debug_bounds("sidebar-titlebar").unwrap();
                    assert!(header.contains(&collapsed.origin));
                    assert!(header.contains(&collapsed.bottom_right()));
                    let leading = cx.debug_bounds("strip-titlebar-leading").unwrap();
                    assert!(!leading.contains(&collapsed.center()));
                    for selector in ["titlebar-back", "titlebar-forward"] {
                        let button = cx.debug_bounds(selector).unwrap();
                        assert!(leading.contains(&button.origin));
                        assert!(button.right() <= leading.right());
                        assert!(button.bottom() <= leading.bottom());
                        assert!(button.left() >= header.right());
                    }
                }
                cx.simulate_click(expanded.center(), Modifiers::default());
                draw(cx);
                assert!(view.read_with(cx, |view, _| view.sidebar_visible));
                assert_eq!(cx.debug_bounds("toggle-sidebar").unwrap(), expanded);
            }
        }
    }
}
