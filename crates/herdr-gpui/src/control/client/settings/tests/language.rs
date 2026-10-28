#![allow(clippy::unwrap_used)]
use super::super::language::{local, text};
use herdr_i18n::{Language, Pseudo};

#[test]
fn without_the_app_status_resolves_the_setting_as_it_would() {
    // No language but English has messages yet, so a tag falls back.
    let french = local(&Language::Locale("fr-CA".parse().unwrap()));
    assert_eq!(
        (french.configured.as_str(), french.locale.as_str()),
        ("fr-CA", "en-US")
    );
    let bidi = local(&Language::Locale(Pseudo::Bidi.locale()));
    assert_eq!(
        (bidi.locale.as_str(), bidi.direction.as_str()),
        ("en-XB", "rtl")
    );
    assert_eq!(local(&Language::System).configured, "system");
}

#[test]
fn status_names_the_setting_the_locale_and_a_pseudo_locale() {
    let mut info = local(&Language::System);
    info.locale = "en-US".into();
    assert_eq!(
        text(&info, false),
        "language = system\nHerdr UI would show en-US (ltr)"
    );
    info.pseudo = Some("en-XA".into());
    info.locale = "en-XA".into();
    assert_eq!(
        text(&info, true),
        "language = system\nHerdr UI shows en-XA (ltr), the QA pseudo-locale en-XA"
    );
}
