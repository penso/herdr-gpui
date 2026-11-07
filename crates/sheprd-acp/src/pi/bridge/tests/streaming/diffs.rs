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
fn an_edit_too_large_to_preview_gets_its_diff_back_when_it_finishes() {
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
        assert_eq!(diff_count(&finished[0]), 1);
        assert_eq!(texts(&finished[0]), ["saved"]);
    }
}

#[test]
fn an_edit_too_large_for_an_entry_stays_omitted_and_within_it() {
    let limit = crate::project::MAX_ENTRY_TEXT;
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    start(
        &mut bridge,
        "edit",
        "edit",
        json!({
            "path": "a.rs", "edits": [{"oldText": "a".repeat(limit), "newText": "b"}],
        }),
    );
    partial(&mut bridge, "edit", "edit", "editing");
    let finished = end(&mut bridge, "edit", "edit", &"c".repeat(limit * 2), false);
    assert_eq!(diff_count(&finished[0]), 0);
    let texts = texts(&finished[0]);
    assert!(
        texts
            .iter()
            .any(|text| text.contains("exceeds live preview limits"))
    );
    assert!(texts.iter().map(|text| text.len()).sum::<usize>() <= limit);
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

#[test]
fn a_huge_command_never_costs_a_failure_its_output() {
    let limit = crate::project::MAX_ENTRY_TEXT;
    let command = "a\n".repeat(limit);
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    start(&mut bridge, "c", "bash", json!({"command": command}));
    partial(&mut bridge, "c", "bash", "working");
    let failed = end(&mut bridge, "c", "bash", "exit code 2", true);
    let texts = texts(&failed[0]);
    assert_eq!(texts.last().copied(), Some("exit code 2"));
    assert!(texts.iter().map(|text| text.len()).sum::<usize>() <= limit);
}

#[test]
fn a_huge_command_keeps_its_preview_when_the_final_result_has_none() {
    let limit = crate::project::MAX_ENTRY_TEXT;
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    start(
        &mut bridge,
        "c",
        "bash",
        json!({"command": "a\n".repeat(limit)}),
    );
    partial(&mut bridge, "c", "bash", "latest");
    let done = bridge.on_harness(&json!({"type":"tool_execution_end","toolCallId":"c",
        "toolName":"bash","result":{},"isError":false}));
    assert_eq!(texts(&done[0]).last().copied(), Some("latest"));
}

#[test]
fn a_final_result_uses_all_the_room_its_input_leaves() {
    let limit = crate::project::MAX_ENTRY_TEXT;
    let output = "o".repeat(limit - 16);
    let mut bridge = opened();
    prompt(&mut bridge, 2);
    start(&mut bridge, "c", "bash", json!({"command": "one line"}));
    partial(&mut bridge, "c", "bash", "working");
    let done = end(&mut bridge, "c", "bash", &output, false);
    assert_eq!(texts(&done[0]), [output.as_str()]);
}
