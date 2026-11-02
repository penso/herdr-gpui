use super::*;
use herdr_chat_protocol::{Entry, EntryId, EntryKind, Transcript, TurnEnd, TurnSummary};

fn turn(id: u64, ended_ms: Option<u64>) -> Entry {
    Entry {
        id: EntryId(id),
        kind: EntryKind::Turn(TurnSummary {
            duration_ms: 1000,
            ended_ms,
            end: TurnEnd::Done,
            tokens: None,
        }),
    }
}

#[test]
fn a_row_dates_the_chat_by_its_latest_turn() {
    let mut transcript = Transcript::new("Claude");
    assert_eq!(chat::last_turn_ended_ms(&transcript), None);
    transcript.entries = vec![
        turn(1, Some(1_000)),
        Entry {
            id: EntryId(2),
            kind: EntryKind::Agent {
                text: "Done.".into(),
            },
        },
        turn(3, Some(2_000)),
    ];
    assert_eq!(chat::last_turn_ended_ms(&transcript), Some(2_000));
    // A turn from an older program has no time; an earlier turn's would
    // date the chat wrongly.
    transcript.entries.push(turn(4, None));
    assert_eq!(chat::last_turn_ended_ms(&transcript), None);
}
