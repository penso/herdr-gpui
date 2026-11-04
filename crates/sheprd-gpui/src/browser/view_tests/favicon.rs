use super::*;

/// A page's icon replaces its tab's globe, and the globe returns when the
/// page reports none.
#[gpui::test]
fn a_reported_favicon_replaces_the_globe(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    cx.simulate_resize(gpui::size(gpui::px(1600.), gpui::px(600.)));
    let id = cx.update(|_, cx| {
        let scope = scope(&view.read(cx).endpoints[0]);
        let id = Store::update(cx, |store| store.open(scope, "w0", None, None).unwrap());
        view.update(cx, |view, _| view.browser.appear = Default::default());
        id
    });
    draw(cx);
    assert!(cx.debug_bounds("browser-tab-0").is_some());
    assert!(cx.debug_bounds("favicon-0").is_none());

    let pixels = vec![image::Frame::new(image::RgbaImage::new(32, 32))];
    view.update(cx, |view, _| {
        let icon = Arc::new(gpui::RenderImage::new(pixels));
        view.browser.favicons.insert(id, icon);
    });
    draw(cx);
    let icon = cx.debug_bounds("favicon-0").unwrap();
    assert_eq!(icon.size, gpui::size(gpui::px(12.), gpui::px(12.)));

    view.update(cx, |view, _| view.browser.favicons.clear());
    draw(cx);
    assert!(cx.debug_bounds("favicon-0").is_none());
}
