use super::*;

#[test]
fn fl_formats_a_message_with_and_without_arguments() {
    assert_eq!(fl!("palette-filter-commands"), "Commands");
    assert_eq!(
        fl!("commands-focus-agent", number = 2),
        "Focus Agent \u{2068}2\u{2069}"
    );
    assert_eq!(
        fl!("commands-focus-agent", number = 2,),
        fl!("commands-focus-agent", number = 2)
    );
}

/// The check `fl!` runs while compiling, run here to see each refusal.
#[test]
fn the_compile_time_check_refuses_unknown_ids_and_mismatched_arguments() {
    let refused = |id: &'static str, args: &'static [&'static str]| {
        std::panic::catch_unwind(|| check(id, args)).is_err()
    };
    assert!(!refused("palette-hints", &[]));
    assert!(!refused("commands-focus-tab", &["number"]));
    assert!(refused("palette-nothing", &[]));
    assert!(refused("", &[]));
    assert!(refused("commands-focus-tab", &[]));
    assert!(refused("commands-focus-tab", &["number", "extra"]));
    assert!(refused("commands-focus-tab", &["count"]));
    assert!(refused("commands-focus-tab", &["number", "number"]));
    assert!(refused("palette-hints", &["number"]));
}
