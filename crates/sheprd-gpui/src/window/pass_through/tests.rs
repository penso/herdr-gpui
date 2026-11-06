use crate::{
    Command,
    keymap::{Binding, DaemonKeys, Keymap},
    window::SheprdWindow,
};
use gpui::{Entity, Keystroke, TestAppContext, VisualTestContext};
use std::collections::BTreeMap;

/// A window with the prefix on `cmd-j`, one chord, and pass-through on
/// `unlock`, or unbound when that is empty.
fn window<'a>(
    cx: &'a mut TestAppContext,
    unlock: &str,
) -> (Entity<SheprdWindow>, &'a mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    let keys: toml::Table = "prefix = 'cmd+j'\ntoggle_sidebar = 'prefix+cmd+b'"
        .parse()
        .unwrap();
    let overrides = BTreeMap::from([(
        "toggle_pass_through".to_owned(),
        Binding::One(unlock.to_owned()),
    )]);
    view.update(cx, |view, _| {
        view.config.keybindings = Keymap::with_overrides(
            &overrides,
            &Default::default(),
            &DaemonKeys::from_table(Some(&keys)),
        )
        .unwrap();
    });
    cx.update(|window, cx| {
        window.focus(&view.read(cx).focus.clone(), cx);
        window.draw(cx).clear(cx);
    });
    (view, cx)
}

fn press(keystroke: &str, cx: &mut VisualTestContext) -> bool {
    let handled =
        cx.update(|window, cx| window.dispatch_keystroke(Keystroke::parse(keystroke).unwrap(), cx));
    cx.run_until_parked();
    handled
}

fn toggle(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) {
    view.update_in(cx, |view, window, cx| {
        view.command(Command::TogglePassThrough, window, cx);
    });
}

fn on(view: &Entity<SheprdWindow>, cx: &mut VisualTestContext) -> bool {
    view.read_with(cx, |view, _| view.pass_through)
}

#[gpui::test]
fn the_prefix_does_not_arm_until_the_unlock_key_ends_it(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, "ctrl-alt-y");
    toggle(&view, cx);
    assert!(on(&view, cx));

    assert!(!press("cmd-j", cx), "the prefix is the program's now");
    view.read_with(cx, |view, _| assert!(!view.prefix_armed));

    assert!(press("ctrl-alt-y", cx), "the unlock key stops here");
    assert!(!on(&view, cx));

    assert!(press("cmd-j", cx));
    view.read_with(cx, |view, _| assert!(view.prefix_armed));
}

#[gpui::test]
fn it_shows_the_unlock_key_while_on(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, "ctrl-alt-y");
    let unlock = view.read_with(cx, |view, _| view.pass_through_unlock());
    assert_eq!(unlock.as_deref(), Some("ctrl-alt-y"));
    toggle(&view, cx);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("pass-through").is_some());
    press("ctrl-alt-y", cx);
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(cx.debug_bounds("pass-through").is_none());
}

/// With no key to end it, the mode would hold the keyboard.
#[gpui::test]
fn it_refuses_to_start_without_an_unlock_key(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, "");
    toggle(&view, cx);
    assert!(!on(&view, cx));
    view.read_with(cx, |view, _| assert!(view.flash.is_some(), "it says why"));
}

#[gpui::test]
fn starting_ends_a_half_typed_chord_and_resize_mode(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, "ctrl-alt-y");
    press("cmd-j", cx);
    view.update(cx, |view, _| view.resize_mode = true);
    toggle(&view, cx);
    view.read_with(cx, |view, _| {
        assert!(!view.prefix_armed);
        assert!(!view.resize_mode);
    });
}

/// A pane cannot receive it, so the app keeps it, as it keeps quitting.
#[gpui::test]
fn a_key_no_pane_can_receive_is_left_to_the_app(cx: &mut TestAppContext) {
    let (view, cx) = window(cx, "ctrl-alt-y");
    toggle(&view, cx);
    assert!(!press("cmd-y", cx));
    assert!(on(&view, cx));
}
