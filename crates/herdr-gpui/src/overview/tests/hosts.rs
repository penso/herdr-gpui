use super::super::hosts::agents_line;
use crate::devices_overview::Tally;

#[test]
fn host_card_leaves_out_counts_of_none() {
    let tally = |working, idle| Tally {
        working,
        idle,
        ..Tally::default()
    };
    let agents_line = |tally| agents_line(tally).replace(['\u{2068}', '\u{2069}'], "");
    assert_eq!(agents_line(Tally::default()), "No agents");
    assert_eq!(agents_line(tally(0, 1)), "1 agent");
    assert_eq!(agents_line(tally(0, 3)), "3 agents");
    assert_eq!(agents_line(tally(1, 0)), "1 agent · 1 working");
    assert_eq!(agents_line(tally(1, 3)), "4 agents · 1 working");
}
