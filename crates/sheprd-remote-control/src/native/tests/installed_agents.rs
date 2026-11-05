//! Which agents the forms offer: what `sheprd-acp agents` found, or all of
//! them when it could not say.

use crate::native::create::{list_agents, offered, parse_agents};
use sheprd_chat_protocol::Agent;

#[test]
fn a_listing_is_read_line_by_line() {
    assert_eq!(parse_agents("claude\npi\n"), vec![Agent::Claude, Agent::Pi]);
}

#[test]
fn a_name_this_daemon_does_not_know_is_skipped() {
    assert_eq!(parse_agents("newcomer\n  codex  \n\n"), vec![Agent::Codex]);
}

#[test]
fn only_installed_agents_are_offered() {
    assert_eq!(offered(Some(vec![Agent::Claude])), vec![Agent::Claude]);
}

#[test]
fn every_agent_is_offered_when_the_search_failed_or_found_none() {
    assert_eq!(offered(None), Agent::ALL);
    assert_eq!(offered(Some(Vec::new())), Agent::ALL);
}

#[test]
fn a_missing_program_lists_nothing() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(list_agents(&dir.path().join("sheprd-acp")), None);
}

#[cfg(unix)]
mod program {
    use super::*;
    use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};

    fn script(dir: &tempfile::TempDir, body: &str) -> PathBuf {
        let path = dir.path().join("sheprd-acp");
        fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn the_program_is_asked_for_its_agents() {
        let dir = tempfile::tempdir().unwrap();
        let program = script(&dir, r#"[ "$1" = agents ] && echo gemini"#);
        assert_eq!(list_agents(&program), Some(vec![Agent::Gemini]));
    }

    #[test]
    fn a_program_that_fails_says_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let program = script(&dir, "echo claude; exit 2");
        assert_eq!(list_agents(&program), None);
    }
}
