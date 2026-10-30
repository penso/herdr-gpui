use super::*;
use crate::config::preferences::Preference;
use herdr_i18n::{Language, Locale};
use std::sync::{Arc, Mutex};

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

/// System leads, then every language with messages; a configured tag
/// without any stays listed, so the picker shows the setting as it is.
#[test]
fn the_choices_are_system_then_each_language() {
    let languages: Vec<_> = crate::i18n::available()
        .into_iter()
        .map(Language::Locale)
        .collect();
    let choices = pages::language_choices(&Language::System);
    assert_eq!(choices[0], Language::System);
    assert_eq!(choices[1..], languages);
    let missing = Language::Locale("tlh".parse().unwrap());
    let choices = pages::language_choices(&missing);
    assert_eq!(choices.last(), Some(&missing));
    assert_eq!(choices.len(), languages.len() + 2);
}

/// Each language is listed by its own name, whatever the windows show.
#[test]
fn languages_go_by_their_own_names() {
    assert_eq!(crate::i18n::endonym(&Locale::FALLBACK), "English");
    let missing: Locale = "tlh".parse().unwrap();
    assert_eq!(crate::i18n::endonym(&missing), "tlh");
}

#[gpui::test]
fn the_general_page_picks_a_language(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    let edits = Arc::new(Mutex::new(Vec::new()));
    let captured = edits.clone();
    view.update(cx, |view, cx| {
        view.controls.preference_io = Some(controls::preferences::PreferenceIo::recording(
            move |edit| captured.lock().unwrap().push(edit),
        ));
        view.section = Section::General;
        cx.notify();
    });
    cx.simulate_resize(size(px(960.), px(2000.)));
    draw(cx);
    assert!(cx.debug_bounds("general-language").is_some());
    let button = cx
        .debug_bounds("general-language-control")
        .unwrap()
        .center();
    cx.simulate_click(button, Default::default());
    cx.run_until_parked();
    draw(cx);
    let options = view.read_with(cx, |view, _| {
        view.dropdown.as_ref().unwrap().options.clone()
    });
    assert_eq!(
        options.len(),
        pages::language_choices(&Language::System).len()
    );
    assert!(options[0].starts_with("System ("), "{options:?}");
    assert!(
        options.iter().any(|option| option == "English"),
        "{options:?}"
    );
    let option = cx
        .debug_bounds("general-language-control-option-1")
        .unwrap()
        .center();
    cx.simulate_click(option, Default::default());
    cx.run_until_parked();
    assert_eq!(
        edits.lock().unwrap().last(),
        Some(&Preference::Language(Language::Locale(Locale::FALLBACK)))
    );
}

/// The list's choices depend on the setting, so a reload that changes it
/// closes an open list rather than let a stale position pick another
/// language; a reload that keeps it leaves the list open.
#[gpui::test]
fn a_reload_that_changes_the_language_closes_the_list(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(skill_fixture);
    let klingon = Language::Locale("tlh".parse().unwrap());
    view.update(cx, |view, cx| {
        view.config.language = klingon.clone();
        view.section = Section::General;
        cx.notify();
    });
    cx.simulate_resize(size(px(960.), px(2000.)));
    draw(cx);
    let button = cx
        .debug_bounds("general-language-control")
        .unwrap()
        .center();
    for (reloaded, open) in [(klingon.clone(), true), (Language::System, false)] {
        if view.read_with(cx, |view, _| view.dropdown.is_none()) {
            cx.simulate_click(button, Default::default());
            cx.run_until_parked();
            draw(cx);
        }
        assert!(view.read_with(cx, |view, _| view.dropdown.is_some()));
        view.update(cx, |view, cx| {
            let mut loaded = fixture();
            loaded.config.language = reloaded.clone();
            view.apply_loaded(Ok(loaded), cx);
        });
        assert_eq!(
            view.read_with(cx, |view, _| view.dropdown.is_some()),
            open,
            "{reloaded}"
        );
    }
}
