use super::*;

#[test]
fn every_agent_has_a_distinct_key_that_names_it() {
    for agent in Agent::ALL {
        assert_eq!(Agent::from_key(agent.key()), Some(*agent));
        let same = Agent::ALL.iter().filter(|other| other.key() == agent.key());
        assert_eq!(same.count(), 1, "{} is not unique", agent.key());
    }
    assert_eq!(Agent::from_key("cursor"), None);
}

#[test]
fn every_install_skips_a_tool_already_there() {
    for agent in Agent::ALL {
        let script = agent.script();
        assert!(
            script.contains(&format!("command -v {} ", agent.key())),
            "{} reinstalls itself",
            agent.key()
        );
        assert!(script.contains(agent.label()));
        assert!(!script.contains('\0'));
    }
}

/// Run `agent`'s composed setup under `sh -e`, as the setup tab does, with
/// only stub `curl` and `bash` commands on `PATH`.
#[cfg(unix)]
fn run_with_curl(agent: Agent, curl: &str) -> std::process::Output {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    for (name, body) in [("curl", curl), ("bash", "exec /bin/sh")] {
        let path = dir.path().join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let script = compose([agent], None).unwrap().unwrap();
    std::process::Command::new("/bin/sh")
        .args(["-ec", &script])
        .env_clear()
        .env("PATH", dir.path())
        .output()
        .unwrap()
}

#[cfg(unix)]
#[test]
fn a_failed_installer_download_stops_the_setup() {
    for agent in [Agent::Claude, Agent::OpenCode] {
        let output = run_with_curl(agent, "exit 22");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(!output.status.success(), "{} kept going", agent.key());
        assert!(!stdout.contains("Agents installed"), "{stdout}");

        let output = run_with_curl(agent, "echo 'echo ran-installer'");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(output.status.success(), "{}: {output:?}", agent.key());
        assert!(stdout.contains("ran-installer"), "{stdout}");
        assert!(stdout.contains("Agents installed"), "{stdout}");
    }
}
