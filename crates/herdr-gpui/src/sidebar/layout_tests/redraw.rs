use super::*;

#[gpui::test]
fn terminal_redraws_reuse_the_cached_sidebar(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    let renders = |cx: &mut gpui::VisualTestContext| {
        cx.update(|_, cx| view.read(cx).sidebar_view.read(cx).renders)
    };
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let first = renders(cx);
    assert!(first > 0);

    // Terminal output: the window redraws, the rows do not rebuild.
    for _ in 0..3 {
        view.update(cx, |view, cx| view.redraw_terminal(cx));
        cx.update(|window, cx| window.draw(cx).clear(cx));
    }
    assert_eq!(renders(cx), first);

    // Anything else notifies the window, which rebuilds the rows as before.
    view.update(cx, |view, cx| {
        view.sidebar_width = Some(200.);
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert_eq!(renders(cx), first + 1);
    // Debug bounds are only recorded when painted, so read them from a full frame.
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert_eq!(renders(cx), first + 2);
    assert_eq!(
        cx.debug_bounds("sidebar").map(|b| b.size.width),
        Some(px(200.))
    );

    // A hidden sidebar is not built, even for a full frame.
    view.update(cx, |view, cx| {
        view.settings.shared = Some(
            crate::herdr_settings::Settings::parse_text(
                "[ui]\nsidebar_collapsed_mode = 'hidden'\n",
            )
            .unwrap(),
        );
        view.sidebar_visible = false;
        cx.notify();
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert_eq!(renders(cx), first + 2);
}

#[test]
fn child_gutter_lines_land_on_whole_device_pixels() {
    use crate::sidebar::row::{RowTree, tree_lines};
    use gpui::{Bounds, point, size};
    let font = crate::config::FontConfig {
        family: "Menlo".into(),
        size: 12.,
        fallbacks: None,
    };
    for scale in [1., 2., 3.] {
        let row = Bounds::new(point(px(0.), px(244.)), size(px(231.), px(40.)));
        let device = |value: Pixels| f32::from(value) * scale;
        let whole = |value: Pixels| (device(value) - device(value).round()).abs() < 0.001;
        for tree in [RowTree::Child, RowTree::LastChild] {
            let [trunk, tick] = tree_lines(row, tree, &font, 4., scale);
            // Both lines carry the same weight and start on the device grid, so
            // neither is drawn thinner or blurrier than the other.
            assert!(
                (trunk.size.width - tick.size.height).abs() < px(0.01),
                "{scale}"
            );
            assert!(
                (device(trunk.size.width) - scale.round().max(1.)).abs() < 0.01,
                "{scale}"
            );
            for edge in [trunk.left(), trunk.top(), tick.left(), tick.top()] {
                assert!(whole(edge), "{scale}: {edge:?}");
            }
            // The trunk hugs the gutter's leading edge, the tick crosses to the
            // dot at its far edge; neither strays into the label beyond.
            assert_eq!(trunk.left(), tick.left(), "{scale}");
            assert_eq!(trunk.left(), row.left(), "{scale}");
            assert_eq!(tick.right(), row.right(), "{scale}");
            // The tick meets the status dot's middle row.
            let middle = row.top() + px(4. + super::super::line_height(&font) / 2.);
            assert!(
                (tick.center().y - middle).abs() <= px(1. / scale),
                "{scale}"
            );
            // Only a row with a sibling below carries the trunk to the bottom.
            match tree {
                RowTree::Child => assert_eq!(trunk.bottom(), row.bottom(), "{scale}"),
                _ => assert_eq!(trunk.bottom(), tick.bottom(), "{scale}"),
            }
            assert_eq!(trunk.top(), row.top(), "{scale}");
        }
    }
}
