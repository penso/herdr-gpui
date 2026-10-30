use super::*;
use crate::mods::worker::Handle;

#[test]
fn icons_take_the_bar_color() {
    let svg = r#"<svg stroke="currentColor"><path fill="currentColor"/></svg>"#;
    assert_eq!(
        crate::mods::activity::tint(svg, 0x00a1b2c3),
        r##"<svg stroke="#a1b2c3"><path fill="#a1b2c3"/></svg>"##
    );
    // Only the color's low 24 bits are written.
    assert!(crate::mods::activity::tint(svg, 0xff00_0001).contains("#000001"));
}

#[test]
fn the_worker_sends_an_apps_icon_with_its_manifest() {
    let root = tempfile::tempdir().unwrap();
    let dir = write_mod(root.path(), "app", &counter("x"));
    fs::write(
        dir.join("mod.toml"),
        "name = \"app\"\napi = 1\n[activity]\ntitle = \"App\"\nicon = \"icon.svg\"\n",
    )
    .unwrap();
    fs::write(dir.join("icon.svg"), "<svg stroke=\"currentColor\"/>").unwrap();
    let worker = Handle::spawn(dir).unwrap();
    let icon = eventually(|| worker.take().icon);
    assert_eq!(icon.as_deref(), Some("<svg stroke=\"currentColor\"/>"));
}

#[test]
fn a_mod_without_an_app_sends_no_icon() {
    let root = tempfile::tempdir().unwrap();
    let worker = Handle::spawn(write_mod(root.path(), "plain", &counter("x"))).unwrap();
    let icon = eventually(|| worker.take().icon);
    assert_eq!(icon, None);
}
