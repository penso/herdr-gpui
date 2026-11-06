use super::*;

fn diff_count(out: &Out) -> usize {
    update(out)["content"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|part| part["type"] == "diff")
        .count()
}

#[test]
fn oversized_edit_sides_are_omitted_together_in_running_and_final_updates() {
    let limit = crate::project::MAX_ENTRY_TEXT / 4;
    for (old, new) in [
        ("α".repeat(limit), "replacement".into()),
        ("original".into(), "β".repeat(limit)),
        ("a".repeat(limit / 2), "b".repeat(limit / 2)),
    ] {
        let mut bridge = opened();
        prompt(&mut bridge, 2);
        start(
            &mut bridge,
            "edit",
            "edit",
            json!({
                "path": "a.rs", "edits": [{"oldText": old, "newText": new}],
            }),
        );
        let live = partial(&mut bridge, "edit", "edit", "editing");
        assert_eq!(diff_count(&live[0]), 0);
        assert!(
            texts(&live[0])
                .iter()
                .any(|text| text.contains("exceeds live preview limits"))
        );
        let finished = end(&mut bridge, "edit", "edit", "saved", false);
        assert_eq!(diff_count(&finished[0]), 0);
        assert!(
            texts(&finished[0])
                .iter()
                .any(|text| text.contains("exceeds live preview limits"))
        );
        assert!(
            texts(&finished[0])
                .iter()
                .map(|text| text.len())
                .sum::<usize>()
                <= crate::project::MAX_ENTRY_TEXT
        );
    }
}

#[test]
fn omitted_edits_keep_a_notice_and_preserve_smaller_edits_exactly() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    start(
        &mut bridge,
        "edit",
        "edit",
        json!({
            "path": "a.rs", "edits": [
                {"oldText": "a".repeat(crate::project::MAX_ENTRY_TEXT), "newText": "large replacement"},
                {"oldText": "old\n", "newText": "new\n"},
            ],
        }),
    );
    let live = partial(&mut bridge, "edit", "edit", "editing");
    assert_eq!(diff_count(&live[0]), 1);
    let content = update(&live[0])["content"].as_array().unwrap();
    let diff = content.iter().find(|part| part["type"] == "diff").unwrap();
    assert_eq!(diff["oldText"], "old\n");
    assert_eq!(diff["newText"], "new\n");
    assert!(
        texts(&live[0])
            .iter()
            .any(|text| text.contains("exceeds live preview limits"))
    );
}

#[test]
fn authoritative_start_also_never_exposes_truncated_edits_as_diffs() {
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    for (id, name, args) in [
        (
            "edit",
            "edit",
            json!({"path": "a.rs", "edits": [{
                "oldText": "a".repeat(crate::project::MAX_ENTRY_TEXT), "newText": "replacement",
            }]}),
        ),
        (
            "write",
            "write",
            json!({"path": "a.rs", "content": "a".repeat(crate::project::MAX_ENTRY_TEXT)}),
        ),
    ] {
        let started = start(&mut bridge, id, name, args);
        assert_eq!(diff_count(&started[0]), 0);
        assert!(
            texts(&started[0])
                .iter()
                .any(|text| text.contains("exceeds live preview limits"))
        );
    }
}
