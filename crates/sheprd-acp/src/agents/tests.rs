#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

mod installed;
#[cfg(unix)]
mod launch;

#[cfg(unix)]
fn install(dir: &Path, name: &str) {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[cfg(unix)]
#[test]
fn the_direct_adapter_wins_over_npx() {
    let dir = tempfile::tempdir().unwrap();
    install(dir.path(), "npx");
    let path = dir.path().as_os_str();
    let (program, args) = command(Agent::Claude, Some(path));
    assert_eq!(program, dir.path().join("npx").into_os_string());
    assert_eq!(args, ["-y", "@agentclientprotocol/claude-agent-acp"]);
    install(dir.path(), "claude-agent-acp");
    let (program, args) = command(Agent::Claude, Some(path));
    assert_eq!(
        program,
        dir.path().join("claude-agent-acp").into_os_string()
    );
    assert!(args.is_empty());
}

#[cfg(unix)]
#[test]
fn non_executable_files_are_not_programs() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("gemini"), "").unwrap();
    assert_eq!(find_program("gemini", Some(dir.path().as_os_str())), None);
}

#[test]
fn a_missing_agent_is_named_by_its_last_candidate() {
    let (program, args) = command(Agent::Opencode, None);
    assert_eq!(program, "opencode");
    assert_eq!(args, ["acp"]);
}
