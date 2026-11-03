use super::*;
use herdr_i18n::{Locale, Pseudo};

/// Windows' taskbar speaks the count, so switching the language publishes
/// the same count again, described in the new language.
#[gpui::test]
fn a_language_switch_publishes_the_count_again(cx: &mut gpui::TestAppContext) {
    crate::i18n::show(&Locale::FALLBACK);
    let handle = cx.add_window(fixture_window);
    let sync_now = |cx: &mut gpui::TestAppContext| {
        handle
            .update(cx, |view, window, cx| {
                install(cx);
                sync(window.window_handle().window_id(), &view.endpoints, cx);
            })
            .unwrap();
    };
    sync_now(cx);
    cx.read_global::<Badge, _>(|badge, _| {
        assert_eq!(badge.published_in, Some(Locale::FALLBACK));
    });
    crate::i18n::show(&Pseudo::Accented.locale());
    sync_now(cx);
    cx.read_global::<Badge, _>(|badge, _| {
        assert_eq!(badge.published, Some(0));
        assert_eq!(badge.published_in, Some(Pseudo::Accented.locale()));
    });
    crate::i18n::show(&Locale::FALLBACK);
}
