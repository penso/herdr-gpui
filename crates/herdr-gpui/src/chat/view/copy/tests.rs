use super::*;

#[core::prelude::v1::test]
fn rich_copy_buttons_name_english_messages() {
    for copy in [ANSWERS, CODE, LATEX] {
        for id in [copy.label, copy.copied] {
            assert_eq!(crate::i18n::takes(id), Some(&[][..]), "{id}");
        }
    }
}
