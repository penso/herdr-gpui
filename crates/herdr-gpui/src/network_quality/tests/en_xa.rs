use super::*;

#[test]
fn every_tooltip_line_is_a_message_under_en_xa() {
    use herdr_i18n::{Locale, Pseudo};
    let mut reading = Reading::default();
    reading.apply(start(), reply(31));
    reading.apply(start(), lost());
    crate::i18n::show(&Pseudo::Accented.locale());
    let text = crate::network_quality::render::details(&reading);
    crate::i18n::show(&Locale::FALLBACK);
    for line in text.lines() {
        assert!(herdr_i18n::pseudo::is_marked(line), "{line}");
    }
}
