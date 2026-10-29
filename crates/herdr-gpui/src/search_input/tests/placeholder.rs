use super::*;
use crate::i18n::{self, fl};
use herdr_i18n::{Language, Pseudo};

#[gpui::test]
fn a_localized_placeholder_follows_the_locale(cx: &mut gpui::TestAppContext) {
    let (localized, fixed) = cx.update(|cx| {
        i18n::install(&Language::System, cx);
        let localized = cx.new(|cx| {
            let mut input = SearchInput::new(cx);
            input.set_placeholder_with(|| fl!("settings-search-placeholder"), cx);
            input
        });
        let fixed = cx.new(|cx| {
            let mut input = SearchInput::new(cx);
            input.set_placeholder_with(|| fl!("settings-search-placeholder"), cx);
            // A plain placeholder replaces the localized one for good.
            input.set_placeholder("/path/to/file", cx);
            input
        });
        (localized, fixed)
    });
    let english = cx.update(|_| fl!("settings-search-placeholder"));
    assert_eq!(
        localized.read_with(cx, |input, _| input.placeholder().to_owned()),
        english
    );

    cx.update(|cx| i18n::toggle_pseudo(Pseudo::Accented, cx));
    let shown = localized.read_with(cx, |input, _| input.placeholder().to_owned());
    assert_ne!(shown, english);
    assert!(herdr_i18n::pseudo::is_marked(&shown), "{shown}");
    assert_eq!(
        fixed.read_with(cx, |input, _| input.placeholder().to_owned()),
        "/path/to/file"
    );

    cx.update(|cx| i18n::toggle_pseudo(Pseudo::Accented, cx));
    assert_eq!(
        localized.read_with(cx, |input, _| input.placeholder().to_owned()),
        english
    );
}
