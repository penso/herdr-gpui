use super::*;

#[test]
fn direction_follows_the_script() {
    for (tag, direction) in [
        ("en-US", Direction::Ltr),
        ("zh-Hans", Direction::Ltr),
        ("he", Direction::Rtl),
        ("ar-EG", Direction::Rtl),
        ("fa", Direction::Rtl),
        ("ur", Direction::Rtl),
        ("az-Arab", Direction::Rtl),
        ("en-XA", Direction::Ltr),
        ("en-XB", Direction::Rtl),
    ] {
        assert_eq!(locale(tag).direction(), direction, "{tag}");
    }
}

#[test]
fn pseudo_locales_are_recognized_by_tag() {
    assert_eq!(locale("en-XA").pseudo(), Some(Pseudo::Accented));
    assert_eq!(locale("en-xb").pseudo(), Some(Pseudo::Bidi));
    assert_eq!(locale("en-US").pseudo(), None);
    for pseudo in Pseudo::ALL {
        assert_eq!(pseudo.locale().pseudo(), Some(pseudo));
    }
}

#[test]
fn language_setting_parses_system_or_a_tag() {
    assert_eq!("system".parse::<Language>().unwrap(), Language::System);
    assert_eq!("System".parse::<Language>().unwrap(), Language::System);
    assert_eq!(
        "pt-BR".parse::<Language>().unwrap(),
        Language::Locale(locale("pt-BR"))
    );
    assert_eq!(Language::System.to_string(), "system");
    assert_eq!(Language::Locale(locale("he-IL")).to_string(), "he-IL");
    let error = "not a tag!".parse::<Language>().unwrap_err();
    assert!(matches!(&error, Error::InvalidLocale { tag, .. } if tag == "not a tag!"));
    assert!(std::error::Error::source(&error).is_some());
}

#[test]
fn language_setting_round_trips_through_serde() {
    let language: Language = serde_json::from_str("\"zh-Hant-TW\"").unwrap();
    assert_eq!(language, Language::Locale(locale("zh-Hant-TW")));
    assert_eq!(serde_json::to_string(&language).unwrap(), "\"zh-Hant-TW\"");
    assert_eq!(
        serde_json::from_str::<Language>("\"system\"").unwrap(),
        Language::default()
    );
    assert!(serde_json::from_str::<Language>("\"_\"").is_err());
}
