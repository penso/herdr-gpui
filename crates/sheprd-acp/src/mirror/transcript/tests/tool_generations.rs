use super::*;

fn running(id: u64) -> Value {
    json!({"session_id":id,"output":"working"})
}

fn exited(code: i64) -> Value {
    json!({"exit_code":code,"output":"finished"})
}

#[test]
fn optional_token_count_is_accepted_only_in_its_exact_header_position() {
    for (name, output, expected) in [
        (
            "exec_command",
            "Chunk ID: abc\nWall time: 1 seconds\nProcess exited with code 9\nOriginal token count: 12345\nFinal output:\ntruncated",
            ToolStatus::Failed,
        ),
        (
            "exec_command",
            "Wall time: 1 seconds\nProcess running with session ID 7\nOriginal token count: 12345\nOutput:\ntruncated",
            ToolStatus::InProgress,
        ),
        (
            "functions.exec",
            "Script failed\nWall time: 1 seconds\nOriginal token count: 12345\nOutput:\ntruncated",
            ToolStatus::Failed,
        ),
        (
            "functions.exec",
            "Script running with cell ID 7\nWall time: 1 seconds\nOriginal token count: 12345\nOutput:\ntruncated",
            ToolStatus::InProgress,
        ),
        (
            "exec_command",
            "Wall time: 1 seconds\nProcess exited with code 9\nOriginal token count: -2\nOutput:\ntext",
            ToolStatus::Completed,
        ),
        (
            "functions.exec",
            "Script failed\nOriginal token count: 12345\nWall time: 1 seconds\nOutput:\ntext",
            ToolStatus::Completed,
        ),
        (
            "functions.exec",
            "Script failed\nWall time: 1 seconds\nOriginal token count: 12345\nOriginal token count: 12345\nOutput:\ntext",
            ToolStatus::Completed,
        ),
    ] {
        let transcript = project(
            TranscriptAgent::Codex,
            &[call("c", name, json!({})), result("c", json!(output))],
        );
        assert_eq!(tool(&transcript, 0).status, expected, "{output}");
    }
}

#[test]
fn overlapping_reused_job_ids_keep_each_poll_with_its_original_generation() {
    let records = [
        call("old", "exec_command", json!({"cmd":"old job"})),
        result("old", running(7)),
        call("old-poll", "write_stdin", json!({"session_id":7})),
        call("new", "exec_command", json!({"cmd":"new job"})),
        result("new", running(7)),
        call("new-poll", "write_stdin", json!({"session_id":7})),
        result("new-poll", exited(2)),
        result("old-poll", exited(0)),
    ];
    let before_old_completion = project(TranscriptAgent::Codex, &records[..records.len() - 1]);
    assert_eq!(
        tool(&before_old_completion, 0).status,
        ToolStatus::InProgress
    );
    assert_eq!(tool(&before_old_completion, 1).status, ToolStatus::Pending);
    assert_eq!(tool(&before_old_completion, 2).status, ToolStatus::Failed);
    let transcript = project(TranscriptAgent::Codex, &records);
    assert_eq!(tool(&transcript, 0).status, ToolStatus::Completed);
    assert_eq!(tool(&transcript, 1).status, ToolStatus::Completed);
    assert_eq!(tool(&transcript, 2).status, ToolStatus::Failed);
    assert_eq!(tool(&transcript, 3).status, ToolStatus::Failed);
}

#[test]
fn duplicate_old_finals_and_running_results_cannot_target_a_reused_job_id() {
    let records = [
        call("old", "exec_command", json!({"cmd":"old job"})),
        result("old", running(7)),
        call("old-poll", "write_stdin", json!({"session_id":7})),
        result("old-poll", exited(0)),
        call("new", "exec_command", json!({"cmd":"new job"})),
        result("new", running(7)),
        result("old-poll", exited(0)),
        result("old", running(7)),
        call("new-poll", "write_stdin", json!({"session_id":7})),
        result("new-poll", exited(2)),
    ];
    let before_new_completion = project(TranscriptAgent::Codex, &records[..records.len() - 2]);
    assert_eq!(
        tool(&before_new_completion, 0).status,
        ToolStatus::Completed
    );
    assert_eq!(
        tool(&before_new_completion, 2).status,
        ToolStatus::InProgress
    );
    let transcript = project(TranscriptAgent::Codex, &records);
    assert_eq!(tool(&transcript, 0).status, ToolStatus::Completed);
    assert_eq!(tool(&transcript, 1).status, ToolStatus::Completed);
    assert_eq!(tool(&transcript, 2).status, ToolStatus::Failed);
}

#[test]
fn a_poll_without_an_observed_origin_cannot_retarget_a_later_launch() {
    let transcript = project(
        TranscriptAgent::Codex,
        &[
            call("poll", "write_stdin", json!({"session_id":7})),
            call("new", "exec_command", json!({"cmd":"new job"})),
            result("new", running(7)),
            result("poll", exited(2)),
        ],
    );
    assert_eq!(tool(&transcript, 0).status, ToolStatus::Failed);
    assert_eq!(tool(&transcript, 1).status, ToolStatus::InProgress);
    assert!(transcript.permissions.is_empty());
}

#[test]
fn a_fresh_poll_does_not_fall_back_to_an_older_generation_after_id_reuse_finishes() {
    let transcript = project(
        TranscriptAgent::Codex,
        &[
            call("old", "exec_command", json!({"cmd":"old job"})),
            result("old", running(7)),
            call("new", "exec_command", json!({"cmd":"new job"})),
            result("new", running(7)),
            call("new-poll", "write_stdin", json!({"session_id":7})),
            result("new-poll", exited(0)),
            call("fresh-poll", "write_stdin", json!({"session_id":7})),
            result("fresh-poll", exited(0)),
        ],
    );
    assert_eq!(tool(&transcript, 0).status, ToolStatus::InProgress);
    assert_eq!(tool(&transcript, 1).status, ToolStatus::Completed);
    assert_eq!(tool(&transcript, 2).status, ToolStatus::Completed);
    assert_eq!(tool(&transcript, 3).status, ToolStatus::Completed);
}

#[test]
fn intermediate_output_preserves_running_and_late_chunks_preserve_terminal_status() {
    let records = [
        call("c", "functions.exec", json!({})),
        result(
            "c",
            json!("Script running with cell ID c7\nWall time: 1 seconds\nOutput:\n"),
        ),
        result("c", json!("An intermediate update")),
        result(
            "c",
            json!("Script failed\nWall time: 1 seconds\nOutput:\nError"),
        ),
        result("c", json!("A late update")),
        result(
            "c",
            json!("Script running with cell ID c7\nWall time: 1 seconds\nOutput:\n"),
        ),
    ];
    let before_final = project(TranscriptAgent::Codex, &records[..3]);
    assert_eq!(tool(&before_final, 0).status, ToolStatus::InProgress);
    let transcript = project(TranscriptAgent::Codex, &records);
    assert_eq!(tool(&transcript, 0).status, ToolStatus::Failed);
}
