use super::*;

const COMMAND: Held = Held {
    command: true,
    shift: false,
    option: false,
    control: false,
};

#[test]
fn a_focused_page_takes_its_keys_but_not_the_app_s() {
    // Typing, arrows, and the page's own shortcuts are the page's.
    for (held, key) in [
        (Held::default(), "a"),
        (Held::default(), "\u{f700}"),
        (COMMAND, "w"),
        (COMMAND, "p"),
        (COMMAND, "a"),
        (COMMAND, "z"),
        (
            Held {
                shift: true,
                ..COMMAND
            },
            "v",
        ),
        (
            Held {
                control: true,
                ..COMMAND
            },
            "q",
        ),
    ] {
        assert_eq!(route(held, key), Route::Page, "{held:?} {key}");
    }
    // The clipboard goes through the edit actions.
    assert_eq!(route(COMMAND, "x"), Route::Edit(Edit::Cut));
    assert_eq!(route(COMMAND, "c"), Route::Edit(Edit::Copy));
    assert_eq!(route(COMMAND, "v"), Route::Edit(Edit::Paste));
    // What macOS keeps for the app and its windows stays with the app.
    for (held, key) in [
        (COMMAND, "q"),
        (COMMAND, "h"),
        (COMMAND, "m"),
        (
            Held {
                option: true,
                ..COMMAND
            },
            "h",
        ),
        (COMMAND, "`"),
        (
            Held {
                shift: true,
                ..COMMAND
            },
            "~",
        ),
    ] {
        assert_eq!(route(held, key), Route::App, "{held:?} {key}");
    }
}
