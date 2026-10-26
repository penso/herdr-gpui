use super::*;

// 2025-01-01 00:00:00 UTC, encoded in the session UUID rather than guessed
// from the process start time (a resumed process can be months newer).
const CODEX_SESSION: &str = "01941f29-7c00-7000-8000-000000000001";

fn codex_options() -> ObserveOptions {
    ObserveOptions {
        session: CODEX_SESSION.into(),
        ..options(TranscriptAgent::Codex)
    }
}

fn rollout(base: &Path, day: &str, name: &str, header: &str) -> std::path::PathBuf {
    let directory = base.join("sessions").join(day);
    fs::create_dir_all(&directory).unwrap();
    log(
        &directory,
        &format!("rollout-{name}-{CODEX_SESSION}.jsonl"),
        header,
    )
}

#[test]
fn daemon_backed_codex_finds_only_its_creation_day_and_timezone_neighbors() {
    for day in ["2024/12/31", "2025/01/01", "2025/01/02"] {
        let dir = tempfile::tempdir().unwrap();
        stat(dir.path(), 456);
        let expected = rollout(&dir.path().join(".codex"), day, "midnight", CODEX_SESSION);
        let found = identity::Identity::at(&codex_options(), dir.path(), Some(dir.path())).unwrap();
        assert_eq!(found.path, expected);
        let mut source = found.source().unwrap();
        assert_eq!(
            source.read().unwrap().unwrap().session.unwrap().id,
            CODEX_SESSION
        );
        stat(dir.path(), 457);
        assert!(
            found.check().is_err(),
            "a reused PID cannot retain the fallback"
        );
    }
}

#[test]
fn codex_fallback_rejects_wrong_headers_duplicates_and_outside_dates() {
    let dir = tempfile::tempdir().unwrap();
    stat(dir.path(), 456);
    let base = dir.path().join(".codex");
    let options = codex_options();
    let wrong = rollout(&base, "2025/01/01", "wrong", "another-session");
    assert!(identity::Identity::at(&options, dir.path(), Some(dir.path())).is_err());
    fs::remove_file(wrong).unwrap();
    rollout(&base, "2025/01/03", "outside", CODEX_SESSION);
    assert!(identity::Identity::at(&options, dir.path(), Some(dir.path())).is_err());
    let expected = rollout(&base, "2025/01/01", "first", CODEX_SESSION);
    assert_eq!(
        identity::Identity::at(&options, dir.path(), Some(dir.path()))
            .unwrap()
            .path,
        expected
    );
    rollout(&base, "2024/12/31", "duplicate", CODEX_SESSION);
    assert!(identity::Identity::at(&options, dir.path(), Some(dir.path())).is_err());
}

#[test]
fn codex_fallback_uses_only_its_running_process_home_override() {
    let dir = tempfile::tempdir().unwrap();
    stat(dir.path(), 456);
    let custom = dir.path().join("private-codex");
    let expected = rollout(&custom, "2025/01/01", "custom", CODEX_SESSION);
    rollout(
        &dir.path().join(".codex"),
        "2025/01/01",
        "ignored",
        CODEX_SESSION,
    );
    fs::write(
        dir.path().join("123/environ"),
        format!(
            "IGNORED_SECRET=private\0CLAUDE_CONFIG_DIR=/ignored\0CODEX_HOME={}\0",
            custom.display()
        ),
    )
    .unwrap();
    let found = identity::Identity::at(&codex_options(), dir.path(), Some(dir.path())).unwrap();
    assert_eq!(found.path, expected);
    assert!(
        identity::Identity::at(
            &options(TranscriptAgent::Codex),
            dir.path(),
            Some(dir.path())
        )
        .is_err(),
        "non-v7 IDs cannot trigger a date lookup"
    );
}

#[test]
fn an_overfull_day_is_rejected_even_when_a_matching_file_was_found() {
    let dir = tempfile::tempdir().unwrap();
    stat(dir.path(), 456);
    let matching = rollout(
        &dir.path().join(".codex"),
        "2025/01/01",
        "match",
        CODEX_SESSION,
    );
    let day = matching.parent().unwrap();
    for index in 0..4096 {
        fs::write(day.join(format!("unrelated-{index}")), []).unwrap();
    }
    let error = match identity::Identity::at(&codex_options(), dir.path(), Some(dir.path())) {
        Ok(_) => panic!("incomplete enumeration was accepted"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("observation limit"));
}
