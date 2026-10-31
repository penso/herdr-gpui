//! A picture goes out in its own `chat_changes`, so the prompt it came
//! with reaches the phone before the picture's bytes do.

use super::*;
use herdr_chat_protocol::{Author, Change, Cursor, Entry, EntryId, EntryKind, Image, ImageType};
use live::split_at_pictures;
use wire::Step;

fn step(seq: u64, kind: EntryKind) -> Step {
    Step {
        cursor: Cursor {
            epoch: uuid::Uuid::from_u128(1),
            seq,
        },
        change: Change::Push {
            entry: Entry {
                id: EntryId(seq),
                kind,
            },
        },
    }
}

fn user(seq: u64) -> Step {
    step(
        seq,
        EntryKind::User {
            text: "look".into(),
            files: 1,
        },
    )
}

fn picture(seq: u64) -> Step {
    step(
        seq,
        EntryKind::Image {
            author: Author::User,
            image: Image {
                data: "/9j/".into(),
                kind: ImageType::Jpeg,
            },
        },
    )
}

fn seqs(batches: &[Vec<Step>]) -> Vec<Vec<u64>> {
    batches
        .iter()
        .map(|batch| batch.iter().map(|step| step.cursor.seq).collect())
        .collect()
}

#[test]
fn a_prompt_goes_out_before_its_pictures() {
    let batches = split_at_pictures(vec![user(1), picture(2), picture(3), user(4)]);
    assert_eq!(seqs(&batches), vec![vec![1], vec![2], vec![3, 4]]);
}

#[test]
fn changes_without_pictures_stay_one_message() {
    let batches = split_at_pictures(vec![user(1), user(2), user(3)]);
    assert_eq!(seqs(&batches), vec![vec![1, 2, 3]]);
}

#[test]
fn nothing_gathered_sends_nothing() {
    assert!(split_at_pictures(Vec::new()).is_empty());
}

#[test]
fn a_leading_picture_needs_no_empty_message_before_it() {
    let batches = split_at_pictures(vec![picture(1), user(2)]);
    assert_eq!(seqs(&batches), vec![vec![1, 2]]);
}
