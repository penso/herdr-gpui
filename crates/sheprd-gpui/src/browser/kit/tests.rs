#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn only_the_kits_own_paths_are_answered() {
    let (css, kind) = asset("/.herdr/kit.css", "").unwrap();
    assert_eq!(kind, "text/css; charset=utf-8");
    assert!(
        String::from_utf8(css)
            .unwrap()
            .contains("@import url(\"theme.css\")")
    );
    assert_eq!(
        asset("/.herdr/kit.js", "").unwrap().1,
        "text/javascript; charset=utf-8"
    );
    assert_eq!(asset("/.herdr/theme.css", "x").unwrap().0, b"x");
    for other in [
        "/kit.css",
        "/.herdr/",
        "/.herdr/../kit.css",
        "/.herdr/kit.css/x",
        "/index.html",
    ] {
        assert!(asset(other, "").is_none(), "{other}");
    }
}

#[test]
fn the_served_theme_follows_the_users_colors() {
    let dark = Theme::default();
    let css = theme_css(&dark);
    assert!(css.contains(":root:not(#hk-untheme) {"));
    assert!(css.contains("color-scheme: dark;"));
    assert!(css.contains(&format!("--hk-bg: #{:06x};", dark.background)));
    assert!(css.contains(&format!("--hk-fg: #{:06x};", dark.foreground)));
    assert!(css.contains(&format!("--hk-raised: #{:06x};", dark.active)));
    let light = Theme::builtin("Catppuccin Latte").unwrap();
    let css = theme_css(&light);
    assert!(css.contains("color-scheme: light;"));
    assert!(css.contains(&format!("--hk-bg: #{:06x};", light.background)));
    // The key changes with the colors, so a theme switch is noticed.
    assert_ne!(theme_key(&dark), theme_key(&light));
    assert_eq!(theme_key(&dark), theme_key(&Theme::default()));
}

#[test]
fn the_kit_never_parses_data_as_html() {
    // Texts go in through textContent; the script has no HTML sinks.
    for sink in [
        "innerHTML",
        "outerHTML",
        "insertAdjacentHTML",
        "document.write",
        "eval(",
    ] {
        assert!(!KIT_JS.contains(sink), "{sink}");
    }
}
