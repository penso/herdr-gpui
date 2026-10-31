use super::*;
use herdr_chat_protocol::Turn;

#[test]
fn reported_turn_start_and_end_use_the_shared_reducer() {
    let mut previous = transcript(&["existing history"]);
    for turn in [
        Some(Turn {
            started_ms: 1700000000000,
        }),
        None,
    ] {
        let mut next = previous.clone();
        next.turn = turn;
        let updates = changes(&previous, &next).unwrap();
        assert!(matches!(&updates[..], [Change::Turn { turn: actual }] if *actual == turn));
        for update in updates {
            previous.apply(&update).unwrap();
        }
        assert_eq!(previous, next);
    }
}
