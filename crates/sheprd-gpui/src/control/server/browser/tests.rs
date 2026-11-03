#![allow(clippy::unwrap_used)]
use super::*;
use crate::control::protocol::Request;

mod scoped_notes;

#[test]
fn pasted_notes_keep_line_breaks_and_lose_other_controls() {
    assert_eq!(
        pastable("Picked: B\n\t- note\u{1b}[201~ls\r\u{202e}x"),
        "Picked: B\n\t- note[201~lsx"
    );
}
