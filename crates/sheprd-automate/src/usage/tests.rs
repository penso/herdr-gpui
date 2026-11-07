use super::*;
use serde_json::json;

#[test]
fn claude_counts_each_message_once() {
    let mut seen = HashSet::new();
    let line = json!({
        "type": "assistant",
        "message": {
            "id": "msg_1",
            "usage": {
                "input_tokens": 10,
                "output_tokens": 20,
                "cache_read_input_tokens": 300,
                "cache_creation_input_tokens": 40
            }
        }
    })
    .to_string();
    let first = claude_line(&line, &mut seen).unwrap();
    assert_eq!(
        first,
        Tokens {
            input: 10,
            output: 20,
            cache_read: 300,
            cache_write: 40,
        }
    );
    assert_eq!(
        claude_line(&line, &mut seen),
        None,
        "the same message again"
    );
    assert_eq!(
        claude_line(r#"{"type":"user","message":{}}"#, &mut seen),
        None
    );
    assert_eq!(claude_line("not json", &mut seen), None);
}

#[test]
fn codex_reads_its_running_total() {
    let value = json!({
        "type": "event_msg",
        "payload": {
            "type": "token_count",
            "info": { "total_token_usage": { "input_tokens": 1000, "cached_input_tokens": 600, "output_tokens": 50 } }
        }
    });
    assert_eq!(
        codex_line(&value),
        Some(Tokens {
            input: 400,
            output: 50,
            cache_read: 600,
            cache_write: 0,
        })
    );
    assert_eq!(
        codex_line(&json!({ "payload": { "type": "agent_message" } })),
        None
    );
}

#[test]
fn claude_project_dirs_match_claude() {
    assert_eq!(
        claude_project_dir("/Users/me/code/app"),
        "-Users-me-code-app"
    );
    assert_eq!(claude_project_dir("/tmp/a b.c"), "-tmp-a-b-c");
    let long = format!("/{}", "x".repeat(250));
    let dir = claude_project_dir(&long);
    assert!(dir.len() > 200 && dir.starts_with(&"-".to_owned()), "{dir}");
    assert_eq!(dir, claude_project_dir(&long), "stable");
}

#[test]
fn claude_tokens_come_from_its_project_files() {
    let home = tempfile::tempdir().unwrap();
    let cwd = "/work/tree";
    let dir = home
        .path()
        .join(".claude/projects")
        .join(claude_project_dir(cwd));
    fs::create_dir_all(&dir).unwrap();
    let line = |id: &str, input: u64| {
        json!({ "type": "assistant", "message": { "id": id, "usage": { "input_tokens": input, "output_tokens": 1 } } })
            .to_string()
    };
    fs::write(
        dir.join("session.jsonl"),
        format!("{}\n{}\n{}\n", line("a", 5), line("a", 5), line("b", 7)),
    )
    .unwrap();
    // CLAUDE_CONFIG_DIR is not set in tests that run this; the fixture home
    // stands in for the user's.
    if std::env::var_os("CLAUDE_CONFIG_DIR").is_some() {
        return;
    }
    let found = tokens("claude", cwd, 0, None, home.path()).unwrap();
    assert_eq!(found.input, 12);
    assert_eq!(found.output, 2);
    assert_eq!(tokens("gemini", cwd, 0, None, home.path()), None);
}
