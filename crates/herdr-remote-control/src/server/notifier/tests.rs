use super::*;
use uuid::Uuid;

#[test]
fn a_copy_skips_the_changes_it_already_holds() {
    let epoch = Uuid::new_v4();
    let at = |seq| Cursor { epoch, seq };
    let base = Base {
        transcript: Transcript::default(),
        cursor: Some(at(5)),
    };
    assert!(base.includes(at(4)));
    assert!(base.includes(at(5)));
    assert!(!base.includes(at(6)));
    // Another run's changes are never in this copy.
    assert!(!base.includes(Cursor {
        epoch: Uuid::new_v4(),
        seq: 1,
    }));
    let unknown = Base {
        transcript: Transcript::default(),
        cursor: None,
    };
    assert!(!unknown.includes(at(1)));
}
