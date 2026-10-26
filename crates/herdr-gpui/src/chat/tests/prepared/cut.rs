use super::*;

#[test]
fn an_answer_cut_twice_ends_with_one_note() {
    // Over the line, word, and per-line bounds at once: the plain fallback
    // notes its long line, and the body clip must not add a second note.
    let text = format!("{}\n{}", "x".repeat(5_000), "a b c\n".repeat(2_100));
    let (prepared, _) = visual::message(&text);
    let body = prepared.body(EntryId(1)).unwrap();
    let Body::Message(parts) = &*body else {
        panic!("message");
    };
    let Some(Part::Text(lines)) = parts.last() else {
        panic!("plain lines");
    };
    let note = lines.last().unwrap();
    assert_eq!(lines.iter().filter(|line| *line == note).count(), 1);
    assert!(note.text.contains("too long to show"));
}
