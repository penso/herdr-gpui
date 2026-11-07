use super::*;

/// At 150% display scale the row's edges snap to device pixels, so the label
/// lands a third of a logical pixel short of its unsnapped width. Windows
/// desktops default to fractional scales; a whole pixel is still a failure.
#[test]
fn child_probe_allows_less_than_one_device_pixel_of_snapping() {
    use crate::config::{Density, LayoutMode};
    let mode = LayoutMode::from(Density::Normal);
    let bounds = Bounds::new(point(px(44.), px(100.)), size(px(179.), px(16.)));
    let mut probe = PaintedText {
        bounds,
        mask: bounds,
        cached: "sidebar-child".into(),
        glyph_text: "sidebar-child".into(),
        width: px(94.),
        clipped: false,
    };
    probe.bounds.size.width = px(268. / 1.5);
    probe.mask = probe.bounds;
    probe.verify_child("sidebar-child", mode, 0., 1.5).unwrap();
    probe.bounds.size.width = px(178.);
    probe.mask = probe.bounds;
    assert!(
        probe
            .verify_child("sidebar-child", mode, 0., 1.5)
            .unwrap_err()
            .to_string()
            .contains("child label width: actual")
    );
}
