use super::*;

fn claude_log(project: &Path) -> std::path::PathBuf {
    fs::create_dir_all(project).unwrap();
    let path = project.join(format!("{SESSION}.jsonl"));
    fs::write(
        &path,
        format!("{{\"type\":\"user\",\"sessionId\":\"{SESSION}\"}}\n"),
    )
    .unwrap();
    path
}

fn open_error(agent: TranscriptAgent, root: &Path) -> Error {
    match identity::Identity::at(&options(agent), root, Some(root)) {
        Ok(_) => panic!("the transcript was identified"),
        Err(error) => error,
    }
}

#[test]
fn a_changed_process_is_its_own_error() {
    let dir = tempfile::tempdir().unwrap();
    stat(dir.path(), 455);
    assert!(matches!(
        open_error(TranscriptAgent::Codex, dir.path()),
        Error::ProcessChanged
    ));
}

#[test]
fn malformed_stat_files_are_typed() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("123")).unwrap();
    fs::write(dir.path().join("123/stat"), "123 no parenthesis\n").unwrap();
    assert!(matches!(
        open_error(TranscriptAgent::Codex, dir.path()),
        Error::InvalidStat
    ));
    fs::write(dir.path().join("123/stat"), "123 (agent) S 1 2\n").unwrap();
    assert!(matches!(
        open_error(TranscriptAgent::Codex, dir.path()),
        Error::MissingStartTime
    ));
    let fields = vec!["x"; 20].join(" ");
    fs::write(
        dir.path().join("123/stat"),
        format!("123 (agent) {fields}\n"),
    )
    .unwrap();
    let error = open_error(TranscriptAgent::Codex, dir.path());
    assert!(matches!(error, Error::InvalidStartTime(_)));
    assert!(std::error::Error::source(&error).is_some());
}

#[test]
fn two_matching_logs_are_ambiguous() {
    let dir = tempfile::tempdir().unwrap();
    stat(dir.path(), 456);
    claude_log(&dir.path().join(".claude/projects/-work-one"));
    claude_log(&dir.path().join(".claude/projects/-work-two"));
    assert!(matches!(
        open_error(TranscriptAgent::Claude, dir.path()),
        Error::Ambiguous
    ));
}

#[test]
fn oversized_environments_are_typed() {
    let dir = tempfile::tempdir().unwrap();
    stat(dir.path(), 456);
    fs::write(dir.path().join("123/environ"), vec![b'x'; 128 * 1024 + 1]).unwrap();
    assert!(matches!(
        open_error(TranscriptAgent::Claude, dir.path()),
        Error::EnvironmentTooLarge
    ));
}

#[test]
fn an_exited_agent_is_not_an_io_failure() {
    let dir = tempfile::tempdir().unwrap();
    stat(dir.path(), 456);
    claude_log(&dir.path().join(".claude/projects/-work"));
    let identity = identity::Identity::at(
        &options(TranscriptAgent::Claude),
        dir.path(),
        Some(dir.path()),
    )
    .unwrap();
    stat(dir.path(), 457);
    assert!(matches!(identity.check(), Err(Error::Exited)));
    fs::remove_dir_all(dir.path().join("123")).unwrap();
    assert!(!identity.process_matches().unwrap());
    assert!(matches!(identity.check(), Err(Error::Exited)));
}

#[cfg(unix)]
#[test]
fn rotation_and_rewrites_are_told_apart() {
    use std::io::Write as _;
    let dir = tempfile::tempdir().unwrap();
    stat(dir.path(), 456);
    let path = claude_log(&dir.path().join(".claude/projects/-work"));
    let identity = identity::Identity::at(
        &options(TranscriptAgent::Claude),
        dir.path(),
        Some(dir.path()),
    )
    .unwrap();
    // Rewriting the pinned inode in place keeps its identity but not its session.
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .write_all(b"{\"type\":\"user\",\"sessionId\":\"another-session-entirely\"}\n")
        .unwrap();
    assert!(matches!(identity.check(), Err(Error::SessionChanged)));
    fs::rename(&path, dir.path().join("moved.jsonl")).unwrap();
    claude_log(&dir.path().join(".claude/projects/-work"));
    assert!(matches!(identity.check(), Err(Error::Rotated)));
}
