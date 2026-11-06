use super::*;
use core::prelude::v1::test;
use sheprd_chat_protocol::EntryId;

fn entries() -> Vec<Entry> {
    let user = |id, text: &str| Entry {
        id: EntryId(id),
        kind: EntryKind::User {
            text: text.into(),
            files: 0,
        },
    };
    let agent = |id| Entry {
        id: EntryId(id),
        kind: EntryKind::Agent { text: "ok".into() },
    };
    vec![
        user(0, "first"),
        agent(1),
        agent(2),
        user(3, "second"),
        agent(4),
    ]
}

fn at(item_ix: usize, offset: f32) -> ListOffset {
    ListOffset {
        item_ix,
        offset_in_item: px(offset),
    }
}

#[test]
fn following_pins_the_latest_prompt_once_it_scrolls_away() {
    assert_eq!(pinned(&entries(), at(4, 0.), true), Some(3));
    assert_eq!(pinned(&entries(), at(3, 10.), true), Some(3));
}

#[test]
fn following_shows_nothing_while_the_latest_prompt_is_in_view() {
    assert_eq!(pinned(&entries(), at(3, 0.), true), None);
    assert_eq!(pinned(&entries(), at(1, 0.), true), None);
}

#[test]
fn reading_back_pins_the_prompt_of_that_turn() {
    assert_eq!(pinned(&entries(), at(2, 0.), false), Some(0));
    assert_eq!(pinned(&entries(), at(4, 0.), false), Some(3));
}

#[test]
fn a_prompt_at_the_very_top_needs_no_pin() {
    assert_eq!(pinned(&entries(), at(0, 0.), false), None);
    assert_eq!(pinned(&entries(), at(3, 0.), false), None);
}

#[test]
fn a_chat_without_prompts_pins_nothing() {
    let agent = Entry {
        id: EntryId(1),
        kind: EntryKind::Agent { text: "ok".into() },
    };
    assert_eq!(pinned(std::slice::from_ref(&agent), at(0, 5.), true), None);
    assert_eq!(pinned(&[], at(0, 0.), false), None);
}

#[test]
fn attachment_lines_are_left_out() {
    let prompt = "look\nat this\n\u{1f4ce} a.png\n\u{1f4ce} b.png";
    assert_eq!(preview(typed(prompt, 2)), "look at this");
}

#[test]
fn a_false_file_count_neither_stalls_nor_eats_the_prompt() {
    assert_eq!(preview(typed("look\nat this", usize::MAX)), "look at this");
}

#[test]
fn a_prompt_runs_on_and_stays_bounded() {
    assert_eq!(preview("one\n\n  two\tthree"), "one two three");
    assert!(preview(&"word ".repeat(10_000)).len() <= MAX_TEXT + 8);
}
