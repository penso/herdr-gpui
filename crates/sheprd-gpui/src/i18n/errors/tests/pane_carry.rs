use super::*;
use crate::pane_menu::carry::Refusal;

#[test]
fn every_pane_carry_refusal_has_a_message() {
    assert_localized(&[Refusal::Busy, Refusal::Underway]);
}
