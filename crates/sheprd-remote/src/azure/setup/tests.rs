use super::*;
use std::cell::Cell;

#[test]
fn a_failed_create_is_undone_and_keeps_its_error() {
    let undone = Cell::new(false);
    let result: std::result::Result<(), &str> =
        undo_on_error(|| Err("install failed"), || undone.set(true));
    assert_eq!(result, Err("install failed"));
    assert!(undone.get());
}

#[test]
fn a_finished_create_is_kept() {
    let undone = Cell::new(false);
    let result: std::result::Result<u8, &str> = undo_on_error(|| Ok(7), || undone.set(true));
    assert_eq!(result, Ok(7));
    assert!(!undone.get());
}

#[test]
fn a_failed_discard_does_not_replace_the_create_error() {
    let tried = Cell::new(false);
    discard_created("VM", "herdr-test", || {
        tried.set(true);
        Err(super::super::Error::DeleteTimeout)
    });
    assert!(tried.get());
}
