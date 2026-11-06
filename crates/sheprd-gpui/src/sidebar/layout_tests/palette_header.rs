use super::*;

/// The query leads the palette, as in Raycast: an enlarged field spanning the
/// panel, with the filters beneath it rather than a title above it.
#[gpui::test]
fn palette_query_spans_the_top_in_a_larger_font(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        crate::bind_keys(cx);
        fixture_window(window, cx)
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_palette(crate::palette::Filter::All, window, cx)
        });
        full_draw(window, cx).clear(cx);
    });
    let line = view.read_with(cx, |view, _| view.config.ui.line_height());
    let search = cx.debug_bounds("theme-search").unwrap();
    let status = cx.debug_bounds("palette-status").unwrap();
    let filter = cx.debug_bounds("palette-filter-all").unwrap();
    assert!(search.size.height >= px(line * 1.5) - px(0.5));
    // Only the field's own inset separates it from the panel's edges.
    assert!(status.size.width - search.size.width <= px(40.5));
    assert!(status.size.width > px(480.));
    assert!(filter.top() >= search.bottom());
}
