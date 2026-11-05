use super::*;
use std::fs;

const CLAUDE_SESSION: &str = "0c6f0c39-3c1e-4b7e-9a51-6d1b8e2d7f10";

fn claude_record(role: &str, text: &str) -> String {
    serde_json::json!({
        "type": role,
        "sessionId": CLAUDE_SESSION,
        "cwd": "/work",
        "message": {"role": role, "content": text},
    })
    .to_string()
}

fn write_claude_log(base: &Path, project: &str, session: &str, records: &[String]) {
    let dir = base.join("projects").join(project);
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join(format!("{session}.jsonl")),
        records.join("\n") + "\n",
    )
    .unwrap();
}

#[test]
fn a_claude_session_reads_back_from_its_own_log() {
    let base = tempfile::tempdir().unwrap();
    write_claude_log(
        base.path(),
        "-work",
        CLAUDE_SESSION,
        &[
            claude_record("user", "hello"),
            claude_record("assistant", "hi there"),
        ],
    );
    let history = read(TranscriptAgent::Claude, CLAUDE_SESSION, base.path()).unwrap();
    assert_eq!(history.entries.len(), 2);
}

#[test]
fn a_session_without_a_log_is_not_found() {
    let base = tempfile::tempdir().unwrap();
    assert!(matches!(
        read(TranscriptAgent::Claude, CLAUDE_SESSION, base.path()),
        Err(Error::NotFound)
    ));
}

#[test]
fn a_log_naming_another_session_is_not_read() {
    let base = tempfile::tempdir().unwrap();
    let other = "11111111-2222-4333-8444-555555555555";
    write_claude_log(
        base.path(),
        "-work",
        other,
        &[claude_record("user", "hello")],
    );
    assert!(matches!(
        read(TranscriptAgent::Claude, other, base.path()),
        Err(Error::NotFound)
    ));
}

#[test]
fn two_logs_of_one_session_are_refused() {
    let base = tempfile::tempdir().unwrap();
    for project in ["-work", "-elsewhere"] {
        write_claude_log(
            base.path(),
            project,
            CLAUDE_SESSION,
            &[claude_record("user", "hello")],
        );
    }
    assert!(matches!(
        read(TranscriptAgent::Claude, CLAUDE_SESSION, base.path()),
        Err(Error::Ambiguous)
    ));
}

#[test]
fn a_codex_session_that_is_not_a_uuid_v7_has_no_log_to_find() {
    let base = tempfile::tempdir().unwrap();
    assert!(matches!(
        read(TranscriptAgent::Codex, CLAUDE_SESSION, base.path()),
        Err(Error::Observe(crate::observe::Error::NotUuidV7))
    ));
}

#[test]
fn the_config_directory_prefers_the_agents_override() {
    let home = Path::new("/home/me");
    assert_eq!(
        config_directory(Some("/custom".into()), Some(home), ".claude"),
        Some(PathBuf::from("/custom"))
    );
    assert_eq!(
        config_directory(Some(OsString::new()), Some(home), ".claude"),
        Some(home.join(".claude"))
    );
    assert_eq!(config_directory(None, None, ".claude"), None);
}
