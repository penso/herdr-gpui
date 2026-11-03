/// The card says the key name rule in its own words, which in English stay
/// the rule `herdr-remote` enforces.
#[test]
fn the_name_rule_message_matches_the_rule_in_english() {
    assert_eq!(
        crate::i18n::english("settings-cloud-railway-name-rule", &[]),
        crate::railway::NAME_RULE
    );
}
