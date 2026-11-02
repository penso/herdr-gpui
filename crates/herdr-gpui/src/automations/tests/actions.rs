use crate::automations::{Error, actions::failure};
use herdr_i18n::{Locale, Pseudo};

#[test]
fn a_failed_command_is_worded_in_the_window_language() {
    let error = Error::NoScript {
        path: "/repo/.herdr/automations/nightly.luau".into(),
        host: "pi".into(),
    };
    crate::i18n::show(&Locale::FALLBACK);
    assert_eq!(
        failure("pi", &error).replace(['\u{2068}', '\u{2069}'], ""),
        "pi: pi has no automation script /repo/.herdr/automations/nightly.luau"
    );

    crate::i18n::show(&Pseudo::Accented.locale());
    let shown = failure("pi", &error);
    crate::i18n::show(&Locale::FALLBACK);
    assert!(herdr_i18n::pseudo::is_marked(&shown), "{shown}");
    assert!(
        !shown.contains("has no automation script"),
        "the error itself is translated: {shown}"
    );
}
