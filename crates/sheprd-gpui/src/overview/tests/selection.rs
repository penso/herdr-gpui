use super::*;
use crate::overview::selection::Selection;

fn order(names: &[&str]) -> Vec<Key> {
    names.iter().map(|name| key(name)).collect()
}

#[test]
fn a_click_toggles_one_row() {
    let mut selection = Selection::default();
    selection.toggle(&key("a"));
    selection.toggle(&key("b"));
    assert_eq!(selection.len(), 2);
    selection.toggle(&key("a"));
    assert!(!selection.contains(&key("a")) && selection.contains(&key("b")));
}

#[test]
fn a_range_runs_from_the_last_toggled_row_either_way() {
    let rows = order(&["a", "b", "c", "d"]);
    let mut down = Selection::default();
    down.toggle(&key("b"));
    down.extend(&key("d"), &rows);
    assert!(["b", "c", "d"].iter().all(|name| down.contains(&key(name))));
    assert!(!down.contains(&key("a")));
    let mut up = Selection::default();
    up.toggle(&key("c"));
    up.extend(&key("a"), &rows);
    assert!(["a", "b", "c"].iter().all(|name| up.contains(&key(name))));
    assert!(!up.contains(&key("d")));
}

#[test]
fn a_range_keeps_what_was_selected_and_adds_no_duplicates() {
    let rows = order(&["a", "b", "c", "d"]);
    let mut selection = Selection::default();
    selection.toggle(&key("d"));
    selection.toggle(&key("a"));
    selection.extend(&key("b"), &rows);
    assert_eq!(selection.len(), 3);
    selection.extend(&key("b"), &rows);
    assert_eq!(selection.len(), 3);
}

#[test]
fn a_range_with_no_listed_anchor_selects_the_row_alone() {
    let rows = order(&["a", "b", "c"]);
    let mut selection = Selection::default();
    selection.extend(&key("c"), &rows);
    assert_eq!(selection.len(), 1);
    assert!(selection.contains(&key("c")));
}

#[test]
fn rows_that_leave_the_list_leave_the_selection_and_their_anchor() {
    let mut selection = Selection::default();
    selection.toggle(&key("a"));
    selection.toggle(&key("b"));
    assert!(!selection.retain(&order(&["a", "b", "c"])));
    assert!(selection.retain(&order(&["a", "c"])));
    assert!(!selection.contains(&key("b")));
    // The anchor was b: c stands alone rather than ranging from nowhere.
    selection.extend(&key("c"), &order(&["a", "c"]));
    assert_eq!(selection.len(), 2);
}
