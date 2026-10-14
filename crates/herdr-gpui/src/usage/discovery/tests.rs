#![allow(clippy::unwrap_used)]
use super::super::probe::HostPath;
use super::*;
use core::prelude::v1::test;

impl super::super::Usage {
    pub(crate) fn queue_installation_discovery_for_test(
        &mut self,
        target: ConnectTarget,
        installed: [bool; PROVIDER_COUNT],
    ) {
        let (requests, _) = mpsc::sync_channel(1);
        let (outgoing, answers) = mpsc::sync_channel(1);
        outgoing
            .send(Answer {
                generation: 1,
                installed: Ok(installed),
            })
            .unwrap();
        self.discovery = Discovery {
            target: Some(target),
            generation: 1,
            worker: Some(Worker { requests, answers }),
            busy: true,
            ..Default::default()
        };
    }

    pub(crate) fn installation_discovery_peer_for_test(
        &mut self,
        target: ConnectTarget,
        installed: [bool; PROVIDER_COUNT],
    ) -> impl FnOnce([bool; PROVIDER_COUNT]) + use<> {
        let (requests, incoming) = mpsc::sync_channel::<Job>(1);
        let (outgoing, answers) = mpsc::sync_channel(1);
        self.discovery = Discovery {
            target: Some(target),
            installed,
            worker: Some(Worker { requests, answers }),
            ..Default::default()
        };
        move |installed| {
            let job = incoming.try_recv().unwrap();
            outgoing
                .send(Answer {
                    generation: job.generation,
                    installed: Ok(installed),
                })
                .unwrap();
        }
    }
}

fn mask(ids: &[&str]) -> [bool; PROVIDER_COUNT] {
    let mut mask = [false; PROVIDER_COUNT];
    for (index, provider) in providers().enumerate() {
        mask[index] = ids.contains(&provider.id());
    }
    mask
}

#[test]
fn newest_scope_fences_old_metadata_answers_and_unchanged_results_stay_quiet() {
    let (requests, incoming) = mpsc::sync_channel(1);
    let (outgoing, answers) = mpsc::sync_channel(1);
    let mut discovery = Discovery {
        worker: Some(Worker { requests, answers }),
        ..Default::default()
    };
    let local = ConnectTarget::Local;
    let remote = ConnectTarget::Ssh {
        target: "test-host".into(),
        session: "test".into(),
    };
    discovery.request(&local);
    let first = incoming.try_recv().unwrap();
    discovery.request(&remote);
    assert!(
        incoming.try_recv().is_err(),
        "only one worker job may run at once"
    );
    outgoing
        .send(Answer {
            generation: first.generation,
            installed: Ok(mask(&["claude"])),
        })
        .unwrap();
    assert!(!discovery.poll());
    assert_eq!(discovery.installed(&local), mask(&[]));
    assert_eq!(discovery.installed(&remote), mask(&[]));
    let current = incoming.try_recv().unwrap();
    assert!(matches!(&current.host, Host::Ssh(host) if host == "test-host"));
    outgoing
        .send(Answer {
            generation: current.generation,
            installed: Ok(mask(&["codex"])),
        })
        .unwrap();
    assert!(discovery.poll());
    assert_eq!(discovery.installed(&remote), mask(&["codex"]));
    assert_eq!(discovery.installed(&local), mask(&[]));
    assert!(!discovery.poll());
}

#[test]
fn metadata_failure_preserves_same_target_evidence_and_retired_workers_stay_idle() {
    let (requests, incoming) = mpsc::sync_channel(1);
    let (outgoing, answers) = mpsc::sync_channel(1);
    let target = ConnectTarget::Local;
    let mut discovery = Discovery {
        target: Some(target.clone()),
        installed: mask(&["claude"]),
        worker: Some(Worker { requests, answers }),
        ..Default::default()
    };
    discovery.request(&target);
    let request = incoming.try_recv().unwrap();
    assert_eq!(discovery.installed(&target), mask(&["claude"]));
    outgoing
        .send(Answer {
            generation: request.generation,
            installed: Err(crate::Error::UsageUnreachable),
        })
        .unwrap();
    assert!(!discovery.poll());
    assert_eq!(discovery.installed(&target), mask(&["claude"]));
    drop(incoming);
    discovery.request(&target);
    assert!(discovery.worker.is_none());
    assert!(discovery.queued.is_none());
    assert!(!discovery.poll());
    assert!(
        discovery.worker.is_none(),
        "a failed request cannot respawn on every poll"
    );
}

#[test]
fn local_detector_checks_file_metadata_not_contents() {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("invalid credentials.json");
    std::fs::write(&file, "not JSON, not a usable sign-in").unwrap();
    let mut exec = Exec::Local;
    let mut detector = Detector::new(&mut exec);
    assert!(detector.file_exists(&HostPath::absolute(file.to_string_lossy())));
    assert!(!detector.file_exists(&HostPath::absolute(directory.path().to_string_lossy())));
    std::fs::remove_file(&file).unwrap();
    assert!(!detector.file_exists(&HostPath::absolute(file.to_string_lossy())));
}

#[cfg(unix)]
#[test]
fn disabled_services_are_found_from_unreadable_files_using_host_locator_overrides() {
    use std::{
        os::unix::fs::PermissionsExt,
        process::{Command, Stdio},
    };
    let directory = tempfile::tempdir().unwrap();
    let codex = directory.path().join("custom codex home");
    let claude = directory.path().join("custom claude home");
    let home = directory.path().join("home");
    std::fs::create_dir_all(&codex).unwrap();
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::create_dir_all(home.join(".gemini")).unwrap();
    for path in [
        codex.join("auth.json"),
        claude.join(".credentials.json"),
        home.join(".gemini/oauth_creds.json"),
    ] {
        std::fs::write(
            &path,
            "invalid and deliberately unreadable credential contents",
        )
        .unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o000)).unwrap();
    }
    let mut command = Command::new("/bin/sh");
    command
        .arg("-s")
        .env_clear()
        .env("HOME", &home)
        .env("CODEX_HOME", &codex)
        .env("CLAUDE_CONFIG_DIR", &claude)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut exec = Exec::Remote(Shell::start(command).unwrap());
    assert_eq!(detect_with(&mut exec), mask(&["codex", "claude", "gemini"]));

    let config = super::super::UsageConfig {
        hide_providers: providers()
            .map(|provider| provider.id().to_owned())
            .collect(),
        ..Default::default()
    };
    super::super::read(
        &Host::Local,
        &config,
        &Default::default(),
        &mut Default::default(),
        |_, _| panic!("disabled providers must not be fetched"),
    )
    .unwrap();
    std::fs::remove_file(codex.join("auth.json")).unwrap();
    assert_eq!(detect_with(&mut exec), mask(&["claude", "gemini"]));
}
