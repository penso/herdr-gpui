use super::edit_shortcut;
use gpui::Modifiers;

#[test]
fn text_fields_edit_with_secondary_or_the_platform_key() {
    assert!(edit_shortcut(Modifiers::secondary_key()));
    assert!(edit_shortcut(Modifiers::command()));
    assert!(!edit_shortcut(Modifiers::none()));
    assert!(!edit_shortcut(Modifiers::alt()));
    let shifted = Modifiers {
        shift: true,
        ..Modifiers::secondary_key()
    };
    assert!(!edit_shortcut(shifted));
    // Ctrl is the desktop's edit key; on macOS it stays with Emacs motions.
    assert_eq!(
        edit_shortcut(Modifiers::control()),
        !cfg!(target_os = "macos")
    );
}
