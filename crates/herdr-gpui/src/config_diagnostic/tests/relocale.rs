use super::*;
use herdr_i18n::{Locale, Pseudo};

fn warning() -> Option<String> {
    Some(crate::i18n::fl!("config-diagnostic-sidebar-gap"))
}

#[test]
fn a_language_switch_writes_the_apps_own_text_again() {
    crate::i18n::show(&Locale::FALLBACK);
    let mut state = ConfigDiagnostic::default();
    state.sync_own(warning().as_deref());
    let english = shown(&state).unwrap();
    assert!(!state.relocale(warning), "nothing to do in the same locale");

    crate::i18n::show(&Pseudo::Accented.locale());
    assert!(state.relocale(warning));
    let accented = shown(&state).unwrap();
    assert_ne!(accented, english);
    assert_eq!(accented, vec![warning().unwrap()]);
    assert!(!state.relocale(warning), "written once per switch");

    crate::i18n::show(&Locale::FALLBACK);
    assert!(state.relocale(warning));
    assert_eq!(shown(&state).unwrap(), english);
}

#[test]
fn the_same_warning_in_another_language_stays_dismissed_and_open() {
    crate::i18n::show(&Locale::FALLBACK);
    let mut state = ConfigDiagnostic::default();
    state.sync_own(warning().as_deref());
    let lines = state.visible().unwrap().clone();
    assert!(state.toggle(&lines));
    crate::i18n::show(&Pseudo::Accented.locale());
    assert!(state.relocale(warning));
    assert!(state.expanded());

    let lines = state.visible().unwrap().clone();
    assert!(state.dismiss(&lines));
    crate::i18n::show(&Locale::FALLBACK);
    assert!(state.relocale(warning));
    assert_eq!(shown(&state), None);
}

#[test]
fn no_text_is_written_while_there_is_none() {
    crate::i18n::show(&Pseudo::Accented.locale());
    let mut state = ConfigDiagnostic::default();
    assert!(!state.relocale(|| panic!("no text to write again")));
    // The daemon's text is not the app's to rewrite.
    state.sync(Some("config.toml invalid; using defaults"));
    crate::i18n::show(&Locale::FALLBACK);
    assert!(!state.relocale(warning));
}
