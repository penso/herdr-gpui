use super::*;
use crate::local_ports::process::{Found, act, check, user};
use std::{os::unix::process::ExitStatusExt, process::Child, time::Duration};
use sysinfo::{Pid, ProcessRefreshKind, ProcessStatus, ProcessesToUpdate, System};

const VICTIM: Identity = Identity {
    pid: 4242,
    started: 1_700_000_000,
};

fn found(started: u64, user: Option<u32>) -> Option<Found> {
    Some(Found {
        started,
        user,
        zombie: false,
    })
}

#[test]
fn a_signal_goes_only_to_the_process_listed() {
    assert_eq!(check(found(VICTIM.started, Some(501)), VICTIM, 501), Ok(()));
    // The OS would not name the owner: the start time still decides.
    assert_eq!(check(found(VICTIM.started, None), VICTIM, 501), Ok(()));
}

#[test]
fn a_reused_pid_or_an_exited_process_is_refused() {
    assert_eq!(
        check(found(VICTIM.started + 1, Some(501)), VICTIM, 501),
        Err(Outcome::Exited)
    );
    assert_eq!(check(None, VICTIM, 501), Err(Outcome::Exited));
    let zombie = Some(Found {
        started: VICTIM.started,
        user: Some(501),
        zombie: true,
    });
    assert_eq!(check(zombie, VICTIM, 501), Err(Outcome::Exited));
}

#[test]
fn another_users_process_is_refused() {
    assert_eq!(
        check(found(VICTIM.started, Some(0)), VICTIM, 501),
        Err(Outcome::Foreign)
    );
}

fn spawn(script: &str) -> Child {
    std::process::Command::new("/bin/sh")
        .args(["-c", script])
        .spawn()
        .unwrap()
}

fn identity(system: &mut System, child: &Child) -> Identity {
    let pid = Pid::from_u32(child.id());
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing(),
    );
    Identity {
        pid: child.id(),
        started: system.process(pid).unwrap().start_time(),
    }
}

fn status(system: &mut System, child: &Child) -> Option<ProcessStatus> {
    let pid = Pid::from_u32(child.id());
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing(),
    );
    system.process(pid).map(sysinfo::Process::status)
}

/// Waits up to a few seconds for whether `child` is `stopped`.
fn reaches(system: &mut System, child: &Child, stopped: bool) -> bool {
    for _ in 0..50 {
        if (status(system, child) == Some(ProcessStatus::Stop)) == stopped {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn pausing_resuming_and_stopping_a_real_process() {
    let mut system = System::new();
    let mut child = spawn("exec sleep 30");
    let victim = identity(&mut system, &child);
    let user = user();
    let grace = Duration::from_secs(3);
    assert_eq!(
        act(&mut system, PortAction::Pause, victim, user, grace),
        Outcome::Signalled
    );
    assert!(reaches(&mut system, &child, true));
    assert_eq!(
        act(&mut system, PortAction::Resume, victim, user, grace),
        Outcome::Signalled
    );
    assert!(reaches(&mut system, &child, false));
    // A paused process still ends: SIGCONT follows SIGTERM.
    act(&mut system, PortAction::Pause, victim, user, grace);
    assert_eq!(
        act(&mut system, PortAction::Stop, victim, user, grace),
        Outcome::Signalled
    );
    assert!(!child.wait().unwrap().success());
    // Its pid now names nothing, or another process: nothing is sent.
    assert_eq!(
        act(&mut system, PortAction::Stop, victim, user, grace),
        Outcome::Exited
    );
}

#[test]
fn a_process_ignoring_sigterm_is_killed_after_its_grace() {
    let mut system = System::new();
    let mut child = spawn("trap '' TERM; while :; do sleep 1; done");
    let victim = identity(&mut system, &child);
    assert_eq!(
        act(
            &mut system,
            PortAction::Stop,
            victim,
            user(),
            Duration::from_millis(300)
        ),
        Outcome::Killed
    );
    let status = child.wait().unwrap();
    assert_eq!(status.signal(), Some(9));
}

#[test]
fn a_stale_start_time_is_never_signalled() {
    let mut system = System::new();
    let mut child = spawn("exec sleep 30");
    let mut victim = identity(&mut system, &child);
    victim.started -= 1;
    assert_eq!(
        act(&mut system, PortAction::Stop, victim, user(), Duration::ZERO),
        Outcome::Exited
    );
    assert!(status(&mut system, &child).is_some());
    child.kill().unwrap();
    child.wait().unwrap();
}
