use super::*;

#[test]
fn saved_tabs_use_the_explicit_desktop_state_context() {
    assert_eq!(
        tabs_path(std::path::Path::new("/desktop/state/remote")),
        Some(std::path::PathBuf::from("/desktop/state/browser-tabs.json"))
    );
}
