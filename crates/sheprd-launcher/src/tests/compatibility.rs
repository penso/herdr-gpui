use super::*;

#[test]
fn either_installed_launcher_prefers_sheprd_and_accepts_the_legacy_gui() {
    let dir = tempfile::tempdir().unwrap();
    let legacy = dir.path().join("herdr-gpui.exe");
    let current = dir.path().join("sheprd.exe");
    std::fs::write(&legacy, b"legacy").unwrap();
    for launcher in ["sheprd-launcher.exe", "herdr-gpui-launcher.exe"] {
        assert_eq!(gui_path(&dir.path().join(launcher)).unwrap(), legacy);
    }
    std::fs::write(&current, b"current").unwrap();
    for launcher in ["sheprd-launcher.exe", "herdr-gpui-launcher.exe"] {
        assert_eq!(gui_path(&dir.path().join(launcher)).unwrap(), current);
    }
}
