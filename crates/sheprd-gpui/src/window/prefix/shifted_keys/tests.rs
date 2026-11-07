use super::*;
use crate::{
    keymap::{DaemonKeys, Keymap},
    window::SheprdWindow,
};
use gpui::{TestAppContext, VisualTestContext};
use std::cell::Cell;

/// What a key types with no modifiers, Shift, Command, and Command and Shift,
/// as macOS keyboard layouts have it.
fn chars(keys: [&str; 4], cmd_layout: bool) -> KeyChars {
    KeyChars::typing(keys, cmd_layout)
}

fn key(modifiers: Modifiers, key: &str) -> Keystroke {
    Keystroke {
        modifiers,
        key: key.into(),
        key_char: None,
    }
}

const NONE: Modifiers = Modifiers {
    control: false,
    alt: false,
    shift: false,
    platform: false,
    function: false,
};
const CMD: Modifiers = Modifiers {
    platform: true,
    ..NONE
};

/// `typed` restored with Shift held on a key that types `layout`.
fn restored(typed: &Keystroke, layout: &KeyChars) -> Option<Keystroke> {
    restore(typed, || Some((0, true)), |_| Some(layout.clone()))
}

fn shifted(modifiers: Modifiers, key: &str) -> Option<Keystroke> {
    Some(Keystroke {
        modifiers: Modifiers {
            shift: true,
            ..modifiers
        },
        key: key.into(),
        key_char: None,
    })
}

/// Each layout as macOS reports it, typed as GPUI names the key with Shift
/// held: from its plain layout, or from its Command layout under Command or
/// on a layout whose letters are not ASCII.
#[test]
fn shift_is_restored_on_the_key_gpui_names_on_each_layout() {
    for (layout, keys, typed, expected) in [
        ("U.S.", ["1", "!", "1", "1"], key(NONE, "!"), "1"),
        ("U.S.", ["/", "?", "/", "/"], key(NONE, "?"), "/"),
        ("Norwegian", ["2", "\"", "2", "2"], key(NONE, "\""), "2"),
        ("Norwegian", ["7", "/", "7", "7"], key(NONE, "/"), "7"),
        ("Norwegian", ["0", "=", "0", "0"], key(NONE, "="), "0"),
        ("German", ["ö", "Ö", "ö", "ö"], key(NONE, "Ö"), "ö"),
        // The digit row types symbols, and Shift the digits.
        ("French", ["&", "1", "&", "1"], key(NONE, "1"), "&"),
        ("Czech", ["ý", "7", "ý", "7"], key(NONE, "7"), "ý"),
        // Command and Shift type what Command does, so GPUI keeps the plain
        // layout's shifted character.
        ("U.S.", ["1", "!", "1", "1"], key(CMD, "!"), "1"),
        ("Norwegian", ["7", "/", "7", "7"], key(CMD, "/"), "7"),
        (
            "Dvorak - QWERTY ⌘",
            ["z", "Z", "/", "?"],
            key(CMD, "?"),
            "/",
        ),
    ] {
        assert_eq!(
            restored(&typed, &chars(keys, false)),
            shifted(typed.modifiers, expected),
            "{layout}: {typed:?}"
        );
    }
    // Russian letters are not ASCII, so GPUI names every key from the
    // Command layout, with or without Command.
    for (keys, typed, expected) in [
        (["2", "\"", "2", "@"], "@", "2"),
        (["7", ".", "7", "&"], "&", "7"),
        (["1", "!", "1", "!"], "!", "1"),
        (["/", "?", "/", "?"], "?", "/"),
    ] {
        for modifiers in [NONE, CMD] {
            let typed = key(modifiers, typed);
            assert_eq!(
                restored(&typed, &chars(keys, true)),
                shifted(modifiers, expected),
                "Russian: {typed:?}"
            );
        }
    }
}

#[test]
fn a_name_gpui_would_not_give_the_key_is_left_alone() {
    // On Russian the plain layout's `"` is not what GPUI names Shift+2.
    let russian_two = chars(["2", "\"", "2", "@"], true);
    assert_eq!(restored(&key(NONE, "\""), &russian_two), None);
    // Under Command, Norwegian Shift+7 is named `/`, not `7`.
    let norwegian_seven = chars(["7", "/", "7", "7"], false);
    assert_eq!(restored(&key(CMD, "7"), &norwegian_seven), None);
    // A letter keeps its Shift in GPUI, so it never needs restoring.
    let letter = chars(["a", "A", "a", "a"], false);
    assert_eq!(restored(&key(NONE, "A"), &letter), None);
}

#[test]
fn the_event_is_read_only_for_a_character_without_shift() {
    let unread = || -> Option<(u16, bool)> { panic!("the event was read") };
    let unlooked = |_| -> Option<KeyChars> { panic!("the layout was read") };
    for typed in [
        Keystroke::parse("shift-1").unwrap(),
        Keystroke::parse("shift-a").unwrap(),
        Keystroke::parse("escape").unwrap(),
        Keystroke::parse("f20").unwrap(),
        key(NONE, ""),
    ] {
        assert_eq!(restore(&typed, unread, unlooked), None, "{typed:?}");
    }
    // Shift is not down, or there is no key event: nothing was dropped.
    assert_eq!(
        restore(&key(NONE, "!"), || Some((18, false)), unlooked),
        None
    );
    assert_eq!(restore(&key(NONE, "!"), || None, unlooked), None);
    assert_eq!(
        restore(&key(NONE, "!"), || Some((18, true)), |_| None),
        None
    );
    // The key code read from the event is the one looked up.
    let looked = Cell::new(None);
    restore(
        &key(NONE, "!"),
        || Some((18, true)),
        |code| {
            looked.set(Some(code));
            None
        },
    );
    assert_eq!(looked.get(), Some(18));
}

/// The U.S. layout's 1 key, read for any key code.
fn us_one(_: u16) -> Option<KeyChars> {
    Some(chars(["1", "!", "1", "1"], false))
}

fn shift_held() -> Option<(u16, bool)> {
    Some((18, true))
}

fn window<'a>(
    table: &str,
    cx: &'a mut TestAppContext,
) -> (gpui::Entity<SheprdWindow>, &'a mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    let table: toml::Table = table.parse().unwrap();
    view.update(cx, |view, _| {
        view.config.keybindings = Keymap::with_overrides(
            &Default::default(),
            &Default::default(),
            &DaemonKeys::from_table(Some(&table)),
        )
        .unwrap();
        view.key_press = shift_held;
        view.key_layout = us_one;
    });
    cx.update(|window, cx| {
        window.focus(&view.read(cx).focus.clone(), cx);
        window.draw(cx).clear(cx);
    });
    (view, cx)
}

fn press(keystroke: &str, cx: &mut VisualTestContext) -> bool {
    let handled = cx.update(|window, cx| {
        window.dispatch_keystroke(crate::keymap::parse_keystroke(keystroke).unwrap(), cx)
    });
    cx.run_until_parked();
    handled
}

fn sidebar(view: &gpui::Entity<SheprdWindow>, cx: &mut VisualTestContext) -> bool {
    view.read_with(cx, |view, _| view.sidebar_visible)
}

/// Through the window's own interceptor: the prefix, then Shift+1 as GPUI
/// names it, runs the chord bound to `prefix+shift+1`.
#[gpui::test]
fn a_shifted_chord_runs_from_the_key_gpui_names(cx: &mut TestAppContext) {
    let (view, cx) = window("prefix = 'cmd+j'\ntoggle_sidebar = 'prefix+shift+1'", cx);
    let visible = sidebar(&view, cx);
    assert!(press("cmd-j", cx));
    assert!(press("!", cx));
    assert_eq!(sidebar(&view, cx), !visible, "the chord ran");
    // Without the prefix, the chord does not run and nothing is armed.
    press("!", cx);
    assert_eq!(sidebar(&view, cx), !visible);
    view.read_with(cx, |view, _| assert!(!view.prefix_armed));
}

/// A chord bound to the symbol itself wins over one bound to the restored key.
#[gpui::test]
fn a_chord_bound_to_the_symbol_wins(cx: &mut TestAppContext) {
    let (view, cx) = window(
        "prefix = 'cmd+j'\ntoggle_sidebar = 'prefix+!'\nhelp = 'prefix+shift+1'",
        cx,
    );
    let visible = sidebar(&view, cx);
    assert!(press("cmd-j", cx));
    assert!(press("!", cx));
    assert_eq!(sidebar(&view, cx), !visible);
    view.read_with(cx, |view, _| assert!(view.menu.page.is_none()));
}

/// After the prefix, a chord on the symbol runs although the restored key
/// is a second prefix, which would otherwise pass the key on.
#[gpui::test]
fn a_chord_on_the_symbol_wins_over_a_restored_prefix(cx: &mut TestAppContext) {
    let (view, cx) = window(
        "prefix = ['cmd+j', 'ctrl+alt+shift+1']\ntoggle_sidebar = 'prefix+ctrl+alt+!'",
        cx,
    );
    let visible = sidebar(&view, cx);
    assert!(press("cmd-j", cx));
    assert!(press("ctrl-alt-!", cx));
    assert_eq!(sidebar(&view, cx), !visible, "the chord ran");
    view.read_with(cx, |view, _| assert!(!view.prefix_armed));
}
