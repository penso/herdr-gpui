#![allow(clippy::unwrap_used)]
use super::*;
use gpui::{TestAppContext, div, prelude::*};

#[test]
fn builtin_themes_read_as_their_own_side() {
    for (name, expected) in [
        ("Default", true),
        ("Nord", true),
        ("Dracula", true),
        ("Catppuccin Mocha", true),
        ("Catppuccin Latte", false),
    ] {
        assert_eq!(dark(&Theme::builtin(name).unwrap()), expected, "{name}");
    }
}

#[test]
fn a_dim_light_theme_is_light_and_a_bright_dark_one_dark() {
    let theme = |background, foreground| Theme {
        background,
        foreground,
        ..Theme::default()
    };
    assert!(!dark(&theme(0x9a9a9a, 0x101010)));
    assert!(dark(&theme(0x5a5a5a, 0xffffff)));
}

struct Empty;

impl Render for Empty {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

#[gpui::test]
fn sync_applies_again_only_when_the_theme_side_changes(cx: &mut TestAppContext) {
    let (_, cx) = cx.add_window_view(|_, _| Empty);
    cx.update(|window, _| {
        let dark = Theme::default();
        let light = Theme::builtin("Catppuccin Latte").unwrap();
        let appearance = window.appearance();
        let mut caption = Caption::default();
        caption.sync(window, &dark);
        assert_eq!(caption.applied, Some((true, appearance)));
        caption.sync(window, &Theme::builtin("Nord").unwrap());
        assert_eq!(caption.applied, Some((true, appearance)));
        caption.sync(window, &light);
        assert_eq!(caption.applied, Some((false, appearance)));
    });
}
