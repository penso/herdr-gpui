use super::*;
use gpui::{Bounds, Pixels, ScaledPixels, VisualTestContext, px, size};

/// The opacities of the painted layers that span all of `region`, bottom
/// first. Smaller quads, such as rows and borders, are left out.
fn layers(cx: &mut VisualTestContext, region: Bounds<Pixels>) -> Vec<f32> {
    cx.update(|window, _| {
        let scale = window.scale_factor();
        let region: Bounds<ScaledPixels> = region.scale(scale);
        let mut quads = window.painted_quads();
        quads.sort_by_key(|quad| quad.order);
        quads
            .iter()
            .filter(|quad| {
                quad.bounds.origin.x <= region.origin.x
                    && quad.bounds.origin.y <= region.origin.y
                    && quad.bounds.bottom_right().x >= region.bottom_right().x
                    && quad.bounds.bottom_right().y >= region.bottom_right().y
            })
            .filter_map(|quad| quad.background.as_solid())
            .map(|color| color.a)
            .filter(|alpha| *alpha > 0.)
            .collect()
    })
}

#[gpui::test]
fn every_region_of_a_translucent_window_is_as_clear_as_the_opacity(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(900.), px(600.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.backdrop.opacity = 0.5;
            cx.notify();
        });
        window.refresh();
        window.draw(cx).clear(cx);
    });

    // A panel over a translucent ground would cover the desktop twice and
    // read as more solid than the rest of the window.
    for selector in ["sidebar", "terminal", "connection-status"] {
        let region = cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} is drawn"));
        assert_eq!(layers(cx, region), [0.5], "{selector}");
    }
    // The active tab is a highlight on the strip, but nothing translucent
    // lies under both the strip and the body beside it.
    let strip = cx.debug_bounds("tab-scroller").unwrap();
    let terminal = cx.debug_bounds("terminal").unwrap();
    assert_eq!(layers(cx, strip.union(&terminal)), Vec::<f32>::new());
    assert_eq!(layers(cx, strip).first(), Some(&0.5));
    // The sidebar's search, resting as its heading, adds nothing over the panel.
    let field = cx.debug_bounds("sidebar-search").unwrap();
    assert_eq!(layers(cx, field), [0.5]);
}
