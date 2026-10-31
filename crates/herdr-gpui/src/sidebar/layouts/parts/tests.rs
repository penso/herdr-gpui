use super::{Line, agent_place};
use gpui::{Empty, div};
use std::borrow::Cow;

fn widths(line: Line<'_>) -> Vec<f32> {
    line.widths()
}

#[test]
fn lines_give_fixed_pieces_their_size_and_share_the_rest() {
    // 100 wide, two 4px gaps: 92 to divide.
    let line = Line::new(100., 4.)
        .fixed(20., Empty)
        .fill(div(), "name")
        .label(div(), "12345", 2., 1.);
    assert_eq!(widths(line), vec![20., 62., 10.]);
}

#[test]
fn short_lines_shrink_labels_before_fixed_pieces_and_never_go_negative() {
    let line = Line::new(30., 4.)
        .fixed(20., Empty)
        .fill(div(), "name")
        .label(div(), "1234567890", 2., 1.);
    assert_eq!(widths(line), vec![20., 0., 2.]);
    let line = Line::new(10., 4.).fixed(20., Empty).fill(div(), "name");
    assert_eq!(widths(line), vec![20., 0.]);
}

#[test]
fn labels_stop_at_their_share_and_fills_split_evenly() {
    let line = Line::new(100., 0.)
        .label(div(), "x".repeat(80), 1., 0.5)
        .fill(div(), "a")
        .spacer();
    assert_eq!(widths(line), vec![50., 25., 25.]);
}

#[test]
fn a_lead_keeps_its_width_ahead_of_labels_that_a_plain_fill_gives_up() {
    let long = || "x".repeat(80);
    let line = Line::new(100., 0.)
        .lead(div(), "grok", 10., 0.4)
        .label(div(), long(), 1., 0.5)
        .label(div(), long(), 1., 0.5);
    assert_eq!(widths(line), vec![40., 50., 10.]);
    let line = Line::new(100., 0.)
        .fill(div(), "grok")
        .label(div(), long(), 1., 0.5)
        .label(div(), long(), 1., 0.5);
    assert_eq!(widths(line), vec![0., 50., 50.]);
}

#[test]
fn a_kept_label_holds_its_width_ahead_of_earlier_labels_but_behind_a_lead() {
    let long = || "x".repeat(80);
    let line = Line::new(100., 0.)
        .lead(div(), "grok", 10., 0.4)
        .label(div(), long(), 1., 0.5)
        .label(div(), long(), 1., 0.5)
        .keep(div(), "idle", 5., 0.3);
    assert_eq!(widths(line), vec![40., 40., 0., 20.]);
    let line = Line::new(30., 0.)
        .lead(div(), "grok", 10., 0.4)
        .keep(div(), "working", 5., 0.9);
    assert_eq!(widths(line), vec![12., 18.]);
}

#[test]
fn agents_join_their_place_only_when_there_is_more_than_a_workspace() {
    assert_eq!(agent_place(None, Some("remote")), None);
    assert!(matches!(
        agent_place(Some(("herdr", None)), None),
        Some(Cow::Borrowed("herdr"))
    ));
    assert_eq!(
        agent_place(Some(("herdr", Some("tab 2"))), Some("remote")).as_deref(),
        Some("remote \u{b7} herdr \u{b7} tab 2")
    );
}
