use super::*;

/// The `language` setting follows a reload in Settings too, which may be
/// the only window left once every main window has closed.
#[gpui::test]
fn a_settings_reload_switches_the_locale_without_a_main_window(cx: &mut TestAppContext) {
    let source = cx.add_window(crate::sidebar::layout_tests::fixture_window);
    cx.update(|cx| {
        cx.set_global(crate::app::InitialAppearance::default());
        crate::i18n::install(&herdr_i18n::Language::System, cx);
        let weak = source.update(cx, |_, _, cx| cx.weak_entity()).unwrap();
        open_fixture(weak, cx);
    });
    source
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    cx.update(|cx| {
        cx.global::<SettingsWindowHandle>()
            .window
            .unwrap()
            .update(cx, |view, _, cx| {
                let accented = herdr_i18n::Pseudo::Accented.locale();
                let mut loaded = fixture();
                loaded.config.language = herdr_i18n::Language::Locale(accented.clone());
                view.apply_loaded(Ok(loaded), cx);
                let current = cx.global::<crate::i18n::CurrentLocale>();
                assert_eq!(current.locale(), &accented);
                assert_eq!(
                    current.configured(),
                    &herdr_i18n::Language::Locale(accented)
                );
            })
            .unwrap();
    });
}
