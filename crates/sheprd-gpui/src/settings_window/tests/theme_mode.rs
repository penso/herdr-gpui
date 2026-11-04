use super::*;
use crate::config::ThemeName;

fn click(cx: &mut VisualTestContext, id: &'static str) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let bounds = cx.debug_bounds(id).unwrap();
    cx.simulate_click(bounds.center(), Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

#[gpui::test]
fn each_mode_segment_is_a_state_the_user_can_choose(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    let weak = cx.update(|cx| source.update(cx, |_, _, cx| cx.weak_entity()).unwrap());
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = SettingsWindow::new(weak, cx);
        view.theme_io = Some(recording_themes(Default::default(), false));
        view.initialize_theme_browser(window, cx);
        view
    });
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    let drafted = |cx: &mut VisualTestContext| {
        view.read_with(cx, |view, _| {
            (view.drafted_theme().to_owned(), view.themes.editing_light)
        })
    };
    assert!(!ThemeName::follows_system(&drafted(cx).0));
    assert!(cx.debug_bounds("theme-side").is_none());
    let lit = |cx: &mut VisualTestContext| view.read_with(cx, |view, cx| view.drafted_is_light(cx));
    assert!(!lit(cx), "the fixture starts on a single dark theme");

    // single dark -> Light: the default light theme, shown at once
    click(cx, "theme-mode-control-0");
    assert_eq!(drafted(cx), ("Catppuccin Latte".to_owned(), true));
    assert!(lit(cx), "Mode shows Light");
    // Light again matches the theme's brightness: nothing changes
    click(cx, "theme-mode-control-0");
    assert_eq!(drafted(cx), ("Catppuccin Latte".to_owned(), true));

    // single light -> Dark: the default dark theme
    click(cx, "theme-mode-control-1");
    assert_eq!(drafted(cx), ("Catppuccin Mocha".to_owned(), false));
    assert!(!lit(cx), "Mode shows Dark");

    // single dark -> System: a pair is created and the side row appears
    click(cx, "theme-mode-control-2");
    let (pair, _) = drafted(cx);
    assert!(ThemeName::follows_system(&pair), "{pair}");
    assert!(cx.debug_bounds("theme-side").is_some());
    assert!(cx.debug_bounds("theme-side-control-0").is_some());
    assert!(cx.debug_bounds("theme-side-control-1").is_some());

    // System -> Light: the pair's light side becomes the single theme
    click(cx, "theme-mode-control-0");
    let (single, _) = drafted(cx);
    assert_eq!(single, ThemeName::side(&pair, true));
    assert!(!ThemeName::follows_system(&single));
    assert!(cx.debug_bounds("theme-side").is_none());
}
