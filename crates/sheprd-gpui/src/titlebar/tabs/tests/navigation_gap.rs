use {
    super::*,
    crate::titlebar::{SIDEBAR_TOGGLE_INSET, navigation::NAVIGATION_MARGIN},
};

#[gpui::test]
fn back_and_forward_keep_the_same_visible_room_before_and_after(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    hide_sidebar(&view, cx);
    let toggle = cx.debug_bounds("toggle-sidebar").unwrap();
    let navigation = cx.debug_bounds("titlebar-navigation").unwrap();
    let tabs = cx.debug_bounds("tab-scroller").unwrap();
    // The toggle's glyph ends short of the toggle's own edge.
    let glyph = toggle.right() - px(SIDEBAR_TOGGLE_INSET);
    assert_eq!(navigation.left() - glyph, px(NAVIGATION_MARGIN));
    assert_eq!(tabs.left() - navigation.right(), px(NAVIGATION_MARGIN));
}

#[gpui::test]
fn back_and_forward_keep_the_same_room_when_they_lead_the_strip(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    // Which layout leaves the toggle in the sidebar's header while Back and
    // Forward lead the strip differs by platform: a narrow sidebar beside the
    // traffic lights, the rail elsewhere.
    let mut checked = 0;
    for rail in [false, true] {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.sidebar_width = Some(160.);
                if rail {
                    view.settings.shared = Some(
                        crate::herdr_settings::Settings::parse_text(
                            "[ui]\nsidebar_collapsed_mode = 'compact'",
                        )
                        .unwrap(),
                    );
                    view.toggle_sidebar();
                }
                cx.notify();
            })
        });
        draw(cx);
        let toggle = cx.debug_bounds("toggle-sidebar").unwrap();
        let Some(leading) = cx.debug_bounds("strip-titlebar-leading") else {
            continue;
        };
        if leading.contains(&toggle.center()) {
            continue;
        }
        let navigation = cx.debug_bounds("titlebar-navigation").unwrap();
        let tabs = cx.debug_bounds("tab-scroller").unwrap();
        assert_eq!(navigation.left() - leading.left(), px(NAVIGATION_MARGIN));
        assert_eq!(tabs.left() - navigation.right(), px(NAVIGATION_MARGIN));
        checked += 1;
    }
    assert!(checked > 0);
}

#[gpui::test]
fn back_keeps_its_room_in_the_expanded_header_with_the_activity_bar(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    for bar in [false, true] {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.config.sidebar_style.activity_bar = bar;
                cx.notify();
            })
        });
        draw(cx);
        let header = cx.debug_bounds("sidebar-titlebar").unwrap();
        let navigation = cx.debug_bounds("titlebar-navigation").unwrap();
        assert!(header.contains(&navigation.center()), "bar: {bar}");
        // The bar takes the toggle to its column; Back stays where the
        // toggle's glyph would leave it, the same room after the clearance.
        let before = match cx
            .debug_bounds("toggle-sidebar")
            .filter(|toggle| header.contains(&toggle.center()))
        {
            Some(toggle) => toggle.right() - px(SIDEBAR_TOGGLE_INSET),
            None => header.left() + px(LEADING),
        };
        assert_eq!(
            navigation.left() - before,
            px(NAVIGATION_MARGIN),
            "bar: {bar}"
        );
        assert_eq!(
            cx.debug_bounds("toggle-sidebar")
                .is_some_and(|toggle| header.contains(&toggle.center())),
            !bar
        );
    }
}
