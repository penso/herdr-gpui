use super::*;

/// The activity bar, shown by default, puts the sidebar beside it, so the
/// native child probe expects the label that much further right.
#[test]
fn native_child_probe_expects_the_label_beside_the_activity_bar() {
    use crate::config::{Density, LayoutMode};
    let mode = LayoutMode::from(Density::Normal);
    let bar = crate::sidebar::activity::BAR_WIDTH;
    let bounds = Bounds::new(point(px(44. + bar), px(100.)), size(px(179.), px(16.)));
    let probe = PaintedText {
        bounds,
        mask: bounds,
        cached: "sidebar-child".into(),
        glyph_text: "sidebar-child".into(),
        width: px(94.),
        clipped: false,
    };
    probe.verify_child("sidebar-child", mode, bar, 1.).unwrap();
    let error = probe
        .verify_child("sidebar-child", mode, 0., 1.)
        .unwrap_err();
    assert!(error.to_string().contains("child label left: actual"));
}
