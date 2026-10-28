use super::*;
use gpui::TestAppContext;

#[gpui::test]
fn the_qa_pseudo_locale_switches_text_and_back(cx: &mut TestAppContext) {
    cx.update(|cx| {
        install(&Language::System, cx);
        assert_eq!(shown_pseudo(), None);
        toggle_pseudo(Pseudo::Accented, cx);
        assert_eq!(shown_pseudo(), Some(Pseudo::Accented));
        let accented = fl!("palette-filter-all");
        assert!(herdr_i18n::pseudo::is_marked(&accented), "{accented}");
        assert_eq!(cx.global::<CurrentLocale>().locale().to_string(), "en-XA");

        // Another pseudo-locale replaces it; choosing it again turns it off.
        toggle_pseudo(Pseudo::Bidi, cx);
        assert_eq!(cx.global::<CurrentLocale>().direction(), Direction::Rtl);
        assert_eq!(fl!("palette-filter-all"), "\u{202E}All\u{202C}");
        toggle_pseudo(Pseudo::Bidi, cx);
        assert_eq!(shown_pseudo(), None);
        assert_eq!(fl!("palette-filter-all"), "All");
        assert_eq!(cx.global::<CurrentLocale>().direction(), Direction::Ltr);
    });
}

#[gpui::test]
fn a_reloaded_setting_switches_the_locale_but_the_qa_one_wins(cx: &mut TestAppContext) {
    cx.update(|cx| {
        install(&Language::System, cx);
        let bidi = Language::Locale(Pseudo::Bidi.locale());
        follow_config(&bidi, cx);
        assert_eq!(cx.global::<CurrentLocale>().configured(), &bidi);
        assert_eq!(shown_pseudo(), Some(Pseudo::Bidi));

        toggle_pseudo(Pseudo::Accented, cx);
        follow_config(&Language::Locale("fr".parse().unwrap()), cx);
        assert_eq!(shown_pseudo(), Some(Pseudo::Accented));
        assert_eq!(
            cx.global::<CurrentLocale>().pseudo(),
            Some(Pseudo::Accented)
        );

        // Turned off, the setting shows again: French has no messages yet.
        toggle_pseudo(Pseudo::Accented, cx);
        let current = cx.global::<CurrentLocale>();
        assert_eq!(current.locale(), &Locale::FALLBACK);
        assert_eq!(fl!("palette-filter-all"), "All");
    });
}
