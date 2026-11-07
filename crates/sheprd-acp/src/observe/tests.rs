#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use crate::mirror::transcript::TranscriptAgent;
use std::{fs, path::Path};

mod codex_fallback;
mod errors;

const SESSION: &str = "01234567-89ab-4cde-8123-456789abcdef";

fn options(agent: TranscriptAgent) -> ObserveOptions {
    ObserveOptions {
        agent,
        session: SESSION.into(),
        pid: 123,
        started: 456,
        pane: Some("w1:p1".into()),
    }
}

fn stat(root: &Path, ticks: u64) {
    fs::create_dir_all(root.join("123/fd")).unwrap();
    let fields = std::iter::once("S".to_owned())
        .chain((0..18).map(|_| "0".into()))
        .chain(std::iter::once(ticks.to_string()))
        .collect::<Vec<_>>()
        .join(" ");
    fs::write(
        root.join("123/stat"),
        format!("123 (agent with ) parentheses) {fields}\n"),
    )
    .unwrap();
}

fn log(root: &Path, name: &str, id: &str) -> std::path::PathBuf {
    let path = root.join(name);
    fs::write(
        &path,
        format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\"}}}}\n"),
    )
    .unwrap();
    path
}

#[cfg(unix)]
#[test]
fn selects_parent_session_among_child_rollout_descriptors() {
    let dir = tempfile::tempdir().unwrap();
    stat(dir.path(), 456);
    let original = log(
        dir.path(),
        &format!("rollout-date-{SESSION}.jsonl"),
        SESSION,
    );
    let child_id = "ffffffff-89ab-4cde-8123-456789abcdef";
    let child = log(
        dir.path(),
        &format!("rollout-newer-{child_id}.jsonl"),
        child_id,
    );
    std::os::unix::fs::symlink(original.clone(), dir.path().join("123/fd/55")).unwrap();
    std::os::unix::fs::symlink(child, dir.path().join("123/fd/56")).unwrap();
    let binding =
        identity::Identity::at(&options(TranscriptAgent::Codex), dir.path(), None).unwrap();
    assert_eq!(binding.path, original);
    stat(dir.path(), 457);
    assert!(
        binding.check().is_err(),
        "PID reuse must not retain the conversation"
    );
}

#[cfg(unix)]
#[test]
fn a_matching_filename_cannot_replace_session_identity() {
    let dir = tempfile::tempdir().unwrap();
    stat(dir.path(), 456);
    let fake = log(
        dir.path(),
        &format!("rollout-date-{SESSION}.jsonl"),
        "another-thread",
    );
    std::os::unix::fs::symlink(fake, dir.path().join("123/fd/55")).unwrap();
    assert!(identity::Identity::at(&options(TranscriptAgent::Codex), dir.path(), None).is_err());
}

#[test]
fn claude_closed_log_is_found_by_exact_session_and_verified_header() {
    let dir = tempfile::tempdir().unwrap();
    stat(dir.path(), 456);
    let project = dir.path().join(".claude/projects/-work-project");
    fs::create_dir_all(&project).unwrap();
    let path = project.join(format!("{SESSION}.jsonl"));
    fs::write(
        &path,
        format!("{{\"type\":\"user\",\"sessionId\":\"{SESSION}\"}}\n"),
    )
    .unwrap();
    let found = identity::Identity::at(
        &options(TranscriptAgent::Claude),
        dir.path(),
        Some(dir.path()),
    )
    .unwrap();
    assert_eq!(found.path, path);
    fs::remove_dir_all(dir.path().join("123")).unwrap();
    assert!(found.check().is_err());
}

#[test]
fn stale_process_is_rejected_before_history_is_opened() {
    let dir = tempfile::tempdir().unwrap();
    stat(dir.path(), 455);
    assert!(identity::Identity::at(&options(TranscriptAgent::Codex), dir.path(), None).is_err());
}

#[test]
fn claude_uses_its_running_process_config_directory() {
    let dir = tempfile::tempdir().unwrap();
    stat(dir.path(), 456);
    let custom = dir.path().join("private-claude");
    let project = custom.join("projects/-work-project");
    fs::create_dir_all(&project).unwrap();
    let path = project.join(format!("{SESSION}.jsonl"));
    fs::write(
        &path,
        format!("{{\"type\":\"user\",\"sessionId\":\"{SESSION}\"}}\n"),
    )
    .unwrap();
    fs::write(
        dir.path().join("123/environ"),
        format!(
            "IGNORED_SECRET=private\0CLAUDE_CONFIG_DIR={}\0",
            custom.display()
        ),
    )
    .unwrap();
    let found = identity::Identity::at(
        &options(TranscriptAgent::Claude),
        dir.path(),
        Some(dir.path()),
    )
    .unwrap();
    assert_eq!(found.path, path);
}

#[test]
fn oversized_process_environments_are_rejected_without_echoing_values() {
    let dir = tempfile::tempdir().unwrap();
    stat(dir.path(), 456);
    fs::write(dir.path().join("123/environ"), vec![b'x'; 128 * 1024 + 1]).unwrap();
    let result = identity::Identity::at(
        &options(TranscriptAgent::Claude),
        dir.path(),
        Some(dir.path()),
    );
    let error = match result {
        Ok(_) => panic!("oversized environment was accepted"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("observation limit"));
    assert!(!error.to_string().contains("xxxx"));
}

#[cfg(unix)]
#[test]
fn replacement_file_cannot_swap_an_attached_conversation() {
    let dir = tempfile::tempdir().unwrap();
    stat(dir.path(), 456);
    let path = log(
        dir.path(),
        &format!("rollout-date-{SESSION}.jsonl"),
        SESSION,
    );
    std::os::unix::fs::symlink(&path, dir.path().join("123/fd/55")).unwrap();
    let identity =
        identity::Identity::at(&options(TranscriptAgent::Codex), dir.path(), None).unwrap();
    let mut source = identity.source().unwrap();
    let saved = dir.path().join("original.jsonl");
    fs::rename(&path, &saved).unwrap();
    log(
        dir.path(),
        path.file_name().unwrap().to_str().unwrap(),
        "other-session",
    );
    assert!(
        identity.check().is_err(),
        "rotation must reconnect and revalidate"
    );
    assert!(
        identity.process_matches().unwrap(),
        "rotation is not agent exit"
    );
    assert_eq!(source.read().unwrap().unwrap().session.unwrap().id, SESSION);
    // An in-place rewrite of the pinned file is rejected as well.
    fs::write(
        saved,
        "{\"type\":\"session_meta\",\"payload\":{\"id\":\"other\"}}\n",
    )
    .unwrap();
    assert!(identity.check().is_err());
}

#[test]
fn cli_accepts_only_explicit_valid_process_and_session_identity() {
    let parse = |args: &[&str]| crate::cli::parse(args.iter().map(std::ffi::OsString::from));
    assert_eq!(
        parse(&["observe", "codex", SESSION, "123", "456", "--pane", "w1:p1"]).unwrap(),
        crate::cli::Parsed::Observe(options(TranscriptAgent::Codex))
    );
    for args in [
        vec!["observe", "codex", "../../secret", "123", "456"],
        vec!["observe", "codex", SESSION, "0", "456"],
        vec!["observe", "codex", SESSION, "123", "0"],
        vec!["observe", "gemini", SESSION, "123", "456"],
    ] {
        assert!(parse(&args).is_err());
    }
}
