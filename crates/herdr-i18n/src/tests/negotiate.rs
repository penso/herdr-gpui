use super::*;

fn ids(tags: &[&str]) -> Vec<LanguageIdentifier> {
    tags.iter().map(|tag| tag.parse().unwrap()).collect()
}

#[test]
fn negotiation_prefers_the_users_order_and_falls_back_to_english() {
    let available = Catalog::available(FILES);
    assert_eq!(available, vec![locale("en-US"), locale("fr")]);
    assert_eq!(
        negotiate(&ids(&["fr-CA", "en-GB"]), &available),
        locale("fr")
    );
    assert_eq!(
        negotiate(&ids(&["de", "en-GB"]), &available),
        locale("en-US")
    );
    assert_eq!(negotiate(&ids(&["he"]), &available), Locale::FALLBACK);
    assert_eq!(
        negotiate::<LanguageIdentifier>(&[], &available),
        Locale::FALLBACK
    );
}

#[test]
fn the_setting_resolves_against_the_system_or_its_own_tag() {
    let available = Catalog::available(FILES);
    let system = ids(&["fr-FR"]);
    assert_eq!(Language::System.resolve(&available, &system), locale("fr"));
    assert_eq!(
        Language::Locale(locale("de")).resolve(&available, &system),
        Locale::FALLBACK
    );
    // Pseudo-locales have no files but are shown when named.
    for pseudo in Pseudo::ALL {
        assert_eq!(
            Language::Locale(pseudo.locale()).resolve(&available, &system),
            pseudo.locale()
        );
    }
}

#[test]
fn system_languages_parse_posix_tags() {
    assert_eq!(
        crate::negotiate::posix_to_bcp47("pt_BR.UTF-8@euro"),
        "pt-BR"
    );
    assert_eq!(crate::negotiate::posix_to_bcp47("en-US"), "en-US");
    // Whatever the OS reports, every entry is a parsed identifier.
    let _ = system_languages();
}
