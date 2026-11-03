use super::*;

#[core::prelude::v1::test]
fn a_path_is_named_by_its_last_two_parts() {
    assert_eq!(
        short_path("crates/sheprd-gpui/src/chat/view.rs"),
        "chat/view.rs"
    );
    assert_eq!(short_path("view.rs"), "view.rs");
    assert_eq!(short_path("src/view.rs"), "src/view.rs");
    assert_eq!(short_path(r"C:\work\src\view.rs"), r"src\view.rs");
}

#[core::prelude::v1::test]
fn the_rail_needs_a_wide_pane() {
    let pane = |width: f32| Bounds::new(point(px(0.), px(0.)), size(px(width), px(400.)));
    assert!(fits(pane(MIN_PANE)));
    assert!(!fits(pane(MIN_PANE - 1.)));
}
