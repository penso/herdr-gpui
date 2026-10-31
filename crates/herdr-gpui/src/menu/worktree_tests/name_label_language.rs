use super::*;
use herdr_i18n::{Locale, Pseudo};

/// The name field's accessible name follows a language change made while
/// the dialog is open, as the caption beside it does, rather than keeping
/// the language the dialog opened in.
#[gpui::test]
fn the_name_label_follows_the_language(cx: &mut gpui::TestAppContext) {
    crate::i18n::show(&Locale::FALLBACK);
    let (view, cx) = cx.add_window_view(sidebar::layout_tests::fixture_window);
    cx.simulate_resize(gpui::size(gpui::px(900.), gpui::px(700.)));
    open_dialog(&view, cx, false, Tab::New);
    let label = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            let source = view.read(cx).menu.worktree.as_ref().unwrap();
            source.name.read(cx).label().map(str::to_owned)
        })
    };
    let english = crate::i18n::english("worktree-source-name", &[]);
    assert_eq!(label(cx).as_deref(), Some(english.as_str()));

    crate::i18n::show(&Pseudo::Accented.locale());
    cx.update(|_, cx| view.update(cx, |_, cx| cx.notify()));
    draw(cx);
    let accented = crate::i18n::text("worktree-source-name", &[]);
    assert_ne!(accented, english);
    assert_eq!(label(cx).as_deref(), Some(accented.as_str()));
    crate::i18n::show(&Locale::FALLBACK);
}
