use super::*;
use crate::overview::{Marks, Sections};

#[test]
fn rows_sort_by_state_in_the_order_they_arrive() {
    let rows = vec![
        row("a", State::Working),
        row("b", State::Blocked),
        row("c", State::Done),
        row("d", State::Idle),
        row("e", State::Failed),
        row("f", State::Working),
    ];
    let sections = Sections::of(rows, None, at(0));
    let names = |rows: &[Row]| {
        rows.iter()
            .map(|row| row.detail.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(names(&sections.needs), ["b", "e"]);
    assert_eq!(names(&sections.working), ["a", "f"]);
    assert_eq!(names(&sections.ready), ["c"]);
    assert_eq!(names(&sections.idle), ["d"]);
    assert_eq!(sections.needing(), 2);
}

#[test]
fn marks_pin_snooze_and_settle_rows() {
    let mut marks = Marks::default();
    marks.toggle_pin(&key("pinned"));
    marks.snooze(&key("later"), State::Blocked, at(500));
    marks.snooze(&key("sooner"), State::Blocked, at(200));
    marks.settle(&key("finished"), State::Done);
    let rows = vec![
        row("pinned", State::Blocked),
        row("later", State::Blocked),
        row("sooner", State::Blocked),
        row("finished", State::Done),
        row("plain", State::Blocked),
    ];
    let sections = Sections::of(rows, Some(&marks), at(100));
    assert_eq!(sections.pinned.len(), 1);
    assert_eq!(sections.needs.len(), 1);
    assert_eq!(sections.settled.len(), 1);
    // Snoozed rows wake soonest first.
    let snoozed: Vec<_> = sections
        .snoozed
        .iter()
        .map(|(row, until)| (row.detail.as_str(), *until))
        .collect();
    assert_eq!(snoozed, [("sooner", at(200)), ("later", at(500))]);
    // A pinned row that needs the user counts; held ones do not.
    assert_eq!(sections.needing(), 2);

    // Once the time comes, the snoozed rows need the user again.
    let rows = vec![row("sooner", State::Blocked), row("later", State::Blocked)];
    let sections = Sections::of(rows, Some(&marks), at(600));
    assert_eq!(sections.needs.len(), 2);
}
