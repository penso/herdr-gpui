use super::*;
use core::prelude::v1::test;

#[test]
fn a_tooltip_standing_in_for_a_title_names_a_mute() {
    assert_eq!(hint("logs".into(), false), "logs");
    assert_eq!(
        hint("logs".into(), true),
        "\u{2068}logs\u{2069} (notifications muted)"
    );
}
