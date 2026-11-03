use crate::control::protocol::work::NoteInfo;

fn note(index: usize) -> NoteInfo {
    NoteInfo {
        host: "local".into(),
        repository: "/src/app/.git".into(),
        branch: format!("branch-{index}"),
        text: "n".repeat(500),
    }
}

#[test]
fn a_long_note_list_is_cut_to_fit_one_answer_newest_first() {
    let all: Vec<_> = (0..512).map(note).collect();
    let (notes, omitted) = super::super::notes::within(all, 64 * 1024);
    assert!(!notes.is_empty());
    assert_eq!(notes.len() + omitted, 512);
    assert!(serde_json::to_vec(&notes).unwrap().len() <= 64 * 1024 + 2);
    assert_eq!(notes[0].branch, "branch-0", "the most recent stay");
    let (few, none) = super::super::notes::within((0..3).map(note).collect(), 64 * 1024);
    assert_eq!((few.len(), none), (3, 0));
}
