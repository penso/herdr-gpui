use super::*;
use herdr_i18n::{Locale, Pseudo};

#[test]
fn the_card_follows_a_language_switch_and_stays_dismissed() {
    crate::i18n::show(&Locale::FALLBACK);
    let mut notice = IconFontNotice::default();
    assert!(notice.observe(true, Some(&frame(1, &["\u{f179}"]))));
    let english = notice.visible().cloned();
    // Idle ticks write nothing again.
    assert!(!notice.observe(true, Some(&frame(1, &["\u{f179}"]))));

    crate::i18n::show(&Pseudo::Accented.locale());
    assert!(notice.observe(true, None));
    let accented = notice.visible().cloned();
    assert_ne!(accented, english);
    assert!(!notice.observe(true, None));

    assert!(accented.is_some_and(|lines| notice.dismiss(&lines)));
    crate::i18n::show(&Locale::FALLBACK);
    notice.observe(true, None);
    assert!(notice.visible().is_none());
}
