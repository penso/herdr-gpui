use super::*;

const FSI: char = '\u{2068}';
const PDI: char = '\u{2069}';

fn args<'a>(pairs: &[(&'a str, FluentValue<'a>)]) -> FluentArgs<'a> {
    let mut args = FluentArgs::new();
    for (name, value) in pairs {
        args.set(*name, value.clone());
    }
    args
}

#[test]
fn messages_come_from_every_feature_file_of_the_language() {
    let catalog = Catalog::new(FILES, Locale::FALLBACK).unwrap();
    assert_eq!(catalog.text("menu-quit"), "Quit");
    assert!(catalog.has("greeting-tabs"));
    assert!(!catalog.has("missing"));
    assert_eq!(catalog.direction(), Direction::Ltr);
}

#[test]
fn placeables_are_isolated_and_plurals_follow_the_language() {
    let catalog = Catalog::new(FILES, Locale::FALLBACK).unwrap();
    assert_eq!(
        catalog.text_with("greeting-hello", &args(&[("name", "שלום".into())])),
        format!("Hello, {FSI}שלום{PDI}!")
    );
    assert_eq!(
        catalog.text_with("greeting-tabs", &args(&[("count", 1.into())])),
        "One tab"
    );
    assert_eq!(
        catalog.text_with("greeting-tabs", &args(&[("count", 3.into())])),
        format!("{FSI}3{PDI} tabs")
    );
}

#[test]
fn english_has_no_isolation_marks() {
    let english = Catalog::english(FILES).unwrap();
    assert_eq!(
        english.text_with("greeting-hello", &args(&[("name", "Ada".into())])),
        "Hello, Ada!"
    );
}

#[test]
fn a_translation_falls_back_to_english_per_message() {
    let french = Catalog::new(FILES, locale("fr")).unwrap();
    assert_eq!(
        french.text_with("greeting-hello", &args(&[("name", "Ada".into())])),
        format!("Bonjour, {FSI}Ada{PDI} !")
    );
    assert_eq!(french.text("menu-quit"), "Quit");
}

#[test]
fn a_locale_without_files_shows_english_under_its_own_name() {
    let german = Catalog::new(FILES, locale("de")).unwrap();
    assert_eq!(german.locale(), &locale("de"));
    assert_eq!(german.text("menu-quit"), "Quit");
}

#[test]
fn pseudo_locales_transform_text_but_not_placeables() {
    let accented = Catalog::new(FILES, Pseudo::Accented.locale()).unwrap();
    let quit = accented.text("menu-quit");
    assert!(pseudo::is_marked(&quit), "{quit}");
    assert_eq!(quit, "[Ɋŭŭīīŧ]");
    let hello = accented.text_with("greeting-hello", &args(&[("name", "Ada".into())]));
    assert!(pseudo::is_marked(&hello), "{hello}");
    assert!(hello.contains(&format!("{FSI}Ada{PDI}")), "{hello}");

    let bidi = Catalog::new(FILES, Pseudo::Bidi.locale()).unwrap();
    assert_eq!(bidi.direction(), Direction::Rtl);
    assert_eq!(bidi.text("menu-quit"), "\u{202E}Quit\u{202C}");
}
