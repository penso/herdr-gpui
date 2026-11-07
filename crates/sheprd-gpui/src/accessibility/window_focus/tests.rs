use super::*;

fn class(name: &CStr) -> &'static AnyClass {
    AnyClass::get(name).unwrap_or_else(|| panic!("{name:?} not registered"))
}

#[test]
fn every_window_class_forwards_focus_after_install() {
    install();
    install();
    for name in WINDOW_CLASSES {
        assert!(
            defines_focus(class(name)),
            "{name:?} does not forward focus"
        );
    }
}

#[test]
fn a_class_inheriting_focus_does_not_define_it() {
    let panel = class(c"NSPanel");
    assert!(
        panel
            .instance_method(sel!(accessibilityFocusedUIElement))
            .is_some()
    );
    assert!(!defines_focus(panel));
}
