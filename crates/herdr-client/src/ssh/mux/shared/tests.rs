use super::*;
use std::{fs, os::unix::fs::PermissionsExt};

const WAIT: Duration = Duration::from_secs(10);

/// A test's files: the fake `ssh`, its dial log, and the masters' pid files.
/// Removed when the test ends.
struct Fixture {
    dir: PathBuf,
}

impl Fixture {
    fn starts(&self) -> usize {
        fs::read_to_string(self.dir.join("log")).map_or(0, |log| log.lines().count())
    }

    /// The pid of the master behind `socket`.
    fn pid(&self, socket: &Path) -> String {
        let name = socket.parent().unwrap().file_name().unwrap();
        let mut file = name.to_owned();
        file.push(".pid");
        fs::read_to_string(self.dir.join(file))
            .unwrap()
            .trim()
            .to_owned()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// A stand-in for `ssh` that logs each dial, then runs `then` with `$socket`
/// set to the control socket path it was given and `$pid` to where it records
/// its pid (outside the socket directory, which the watchdog removes).
/// Readiness is the socket path existing, as a real master creates it once
/// authenticated.
fn fake(name: &str, then: &str) -> (Arc<Registry>, Fixture) {
    fake_with(name, "exit 255", then, IDLE)
}

/// [`fake`] that answers `ssh -O check`, the question whether the user has a
/// master of their own, with `check`, and stops masters after `idle`.
fn fake_with(name: &str, check: &str, then: &str, idle: Duration) -> (Arc<Registry>, Fixture) {
    let dir = std::env::temp_dir().join(format!("herdr-mux-test-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let program = dir.join("ssh");
    fs::write(
        &program,
        format!(
            "#!/bin/sh\ncase $1 in -O) {check};; esac\necho start >> '{fixture}/log'\nfor arg; do case $arg in ControlPath=*) socket=${{arg#ControlPath=}};; esac; done\npid=\"{fixture}/$(basename \"$(dirname \"$socket\")\").pid\"\n{then}\n",
            fixture = dir.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
    (Registry::new(program, idle), Fixture { dir })
}

const READY: &str = r#"echo $$ > "$pid"; : > "$socket"; exec sleep 30"#;

fn open(registry: &Arc<Registry>, lane: Lane) -> Option<Channel> {
    registry.open("devbox", lane, Instant::now() + WAIT, || false)
}

fn alive(pid: &str) -> bool {
    Command::new("kill")
        .args(["-0", pid])
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success()
}

fn wait_until(mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !done() {
        assert!(Instant::now() < deadline, "timed out");
        thread::sleep(POLL);
    }
}

#[test]
fn concurrent_commands_share_one_dial() {
    let (registry, fixture) = fake("concurrent", READY);
    let channels: Vec<Channel> = thread::scope(|scope| {
        let handles: Vec<_> = (0..6)
            .map(|_| scope.spawn(|| open(&registry, Lane::Quick)))
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap().unwrap())
            .collect()
    });
    assert_eq!(fixture.starts(), 1);
    assert!(channels.iter().all(|c| c.socket == channels[0].socket));
}

#[test]
fn a_full_master_sends_the_next_command_direct() {
    let (registry, fixture) = fake("full", READY);
    let mut quick: Vec<Channel> = (0..Lane::Quick.channels())
        .map(|_| open(&registry, Lane::Quick).unwrap())
        .collect();
    assert!(open(&registry, Lane::Quick).is_none());
    quick.pop();
    assert!(open(&registry, Lane::Quick).is_some());
    let bulk: Vec<Channel> = (0..Lane::Bulk.channels())
        .map(|_| open(&registry, Lane::Bulk).unwrap())
        .collect();
    assert!(open(&registry, Lane::Bulk).is_none());
    // Each lane has its own master.
    assert_ne!(quick[0].socket, bulk[0].socket);
    assert_eq!(fixture.starts(), 2);
}

#[test]
fn a_master_that_fails_to_start_is_not_tried_again_until_the_backoff_ends() {
    let (registry, fixture) = fake("failing", "exit 255");
    assert!(open(&registry, Lane::Quick).is_none());
    assert!(open(&registry, Lane::Quick).is_none());
    assert_eq!(fixture.starts(), 1);
    // Once the backoff has passed, one more master is tried.
    let key = ("devbox".to_owned(), Lane::Quick);
    registry.lock().get_mut(&key).unwrap().phase = Phase::Direct {
        until: Instant::now(),
    };
    assert!(open(&registry, Lane::Quick).is_none());
    assert_eq!(fixture.starts(), 2);
}

#[test]
fn a_waiting_command_connects_directly_at_its_deadline() {
    let (registry, fixture) = fake("slow", &format!("sleep 1; {READY}"));
    let started = Instant::now();
    let early = registry.open(
        "devbox",
        Lane::Quick,
        Instant::now() + Duration::from_millis(100),
        || false,
    );
    assert!(early.is_none());
    assert!(started.elapsed() < Duration::from_millis(900));
    // The master it asked for still comes up for the next command.
    assert!(open(&registry, Lane::Quick).is_some());
    assert_eq!(fixture.starts(), 1);
}

#[test]
fn a_cancelled_command_stops_waiting() {
    let (registry, _fixture) = fake("cancelled", &format!("sleep 1; {READY}"));
    let started = Instant::now();
    assert!(
        registry
            .open("devbox", Lane::Quick, Instant::now() + WAIT, || true)
            .is_none()
    );
    assert!(started.elapsed() < Duration::from_millis(900));
}

#[test]
fn a_master_that_ended_is_replaced() {
    let (registry, fixture) = fake("ended", r#"echo $$ > "$pid"; : > "$socket"; sleep 0.3"#);
    let first = open(&registry, Lane::Quick).unwrap();
    let (socket, pid) = (first.socket.clone(), fixture.pid(&first.socket));
    drop(first);
    wait_until(|| !alive(&pid));
    // Its watchdog cleans up after the master, and the master counts as
    // live until the watchdog has exited too.
    wait_until(|| {
        let mut hosts = registry.lock();
        sweep(&mut hosts, Instant::now(), IDLE);
        hosts.values().all(|host| host.masters.is_empty())
    });
    let second = open(&registry, Lane::Quick).unwrap();
    assert_ne!(second.socket, socket);
    assert_eq!(fixture.starts(), 2);
}

#[test]
fn a_retired_master_finishes_its_commands_then_stops() {
    let (registry, fixture) = fake("retired", READY);
    let old = open(&registry, Lane::Quick).unwrap();
    let old_pid = fixture.pid(&old.socket);
    registry.retire("devbox");
    let new = open(&registry, Lane::Quick).unwrap();
    assert_ne!(new.socket, old.socket);
    assert_eq!(fixture.starts(), 2);
    // The retired master still serves the command on it.
    assert!(alive(&old_pid));
    let old_dir = old.socket.parent().unwrap().to_owned();
    drop(old);
    wait_until(|| !alive(&old_pid));
    wait_until(|| !old_dir.exists());
    assert!(alive(&fixture.pid(&new.socket)));
}

#[test]
fn an_idle_master_is_stopped() {
    // Nothing else happens after the last command: the master still stops.
    let idle = Duration::from_millis(500);
    let (registry, fixture) = fake_with("idle", "exit 255", READY, idle);
    let channel = open(&registry, Lane::Quick).unwrap();
    let pid = fixture.pid(&channel.socket);
    drop(channel);
    let dropped = Instant::now();
    wait_until(|| !alive(&pid));
    assert!(dropped.elapsed() >= idle);
    assert!(registry.lock().is_empty());
    wait_until(|| !registry.janitor.load(Ordering::Acquire));
}

#[test]
fn a_used_master_is_not_stopped_while_busy_or_soon_after() {
    let idle = Duration::from_millis(500);
    let (registry, fixture) = fake_with("busy", "exit 255", READY, idle);
    let channel = open(&registry, Lane::Quick).unwrap();
    let pid = fixture.pid(&channel.socket);
    thread::sleep(idle * 2);
    assert!(alive(&pid));
    drop(channel);
    thread::sleep(idle / 2);
    assert!(alive(&pid));
    wait_until(|| !alive(&pid));
}

#[test]
fn a_master_still_starting_when_the_host_is_retired_is_not_used() {
    let (registry, fixture) = fake("retired-starting", &format!("sleep 0.5; {READY}"));
    let waiting = thread::spawn({
        let registry = registry.clone();
        move || open(&registry, Lane::Quick).unwrap()
    });
    wait_until(|| fixture.starts() == 1);
    registry.retire("devbox");
    let channel = waiting.join().unwrap();
    assert_eq!(fixture.starts(), 2);
    let key = ("devbox".to_owned(), Lane::Quick);
    let hosts = registry.lock();
    let masters = &hosts[&key].masters;
    assert_eq!(masters.len(), 1);
    assert_eq!(masters[0].socket, channel.socket);
}

#[test]
fn the_master_ends_when_this_process_goes_away() {
    // A crash kills nothing on its own: only the closed pipe remains. Killing
    // the watchdog shell outright and then closing the pipe stands in for it.
    let (registry, fixture) = fake("orphan", READY);
    let mut master = launch(&registry.program, "devbox", Lane::Quick, 1).unwrap();
    let pid = fixture.pid(&master.socket);
    let mut shell = master.process.child.take().unwrap();
    shell.kill().unwrap();
    shell.wait().unwrap();
    assert!(alive(&pid));
    drop(master.process.stdin.take());
    wait_until(|| !alive(&pid));
}

#[test]
fn masters_are_private_foreground_children_and_channels_never_become_masters() {
    let socket = Path::new("/tmp/herdr-mux-1-1/c");
    let quick = master_args("me@box", Lane::Quick, socket);
    let has = |args: &[String], option: &str| args.iter().any(|arg| arg == option);
    for option in [
        "ControlMaster=yes",
        "ControlPersist=no",
        "ForkAfterAuthentication=no",
        "BatchMode=yes",
        "ControlPath=/tmp/herdr-mux-1-1/c",
        "Compression=yes",
    ] {
        assert!(has(&quick, option), "{option}");
    }
    assert_eq!(quick[..2], ["-N", "-T"]);
    assert_eq!(quick[quick.len() - 2..], ["--", "me@box"]);
    let bulk = master_args("me@box", Lane::Bulk, socket);
    assert!(has(&bulk, "Compression=no"));
    assert!(!has(&bulk, "Compression=yes"));

    let channel = channel_command("me@box", "true", socket);
    let args: Vec<_> = channel
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    // OpenSSH keeps the first value: the private socket, then never a master.
    assert_eq!(args[..2], ["-o", "ControlPath=/tmp/herdr-mux-1-1/c"]);
    assert!(has(&args, "ControlMaster=no"));
    assert_eq!(args[args.len() - 3..], ["--", "me@box", "true"]);
}

#[test]
#[ignore = "requires HERDR_MUX_TEST_SSH, a host reachable with noninteractive SSH"]
fn commands_on_a_real_host_share_one_master() {
    let target = std::env::var("HERDR_MUX_TEST_SSH").unwrap();
    let registry = Registry::new("ssh", IDLE);
    let outputs: Vec<String> = thread::scope(|scope| {
        let handles: Vec<_> = (0..6)
            .map(|_| {
                scope.spawn(|| {
                    let channel = registry
                        .open(&target, Lane::Quick, Instant::now() + WAIT, || false)
                        .unwrap();
                    let output = channel_command(&target, "echo $PPID", &channel.socket)
                        .stdin(Stdio::null())
                        .stderr(Stdio::null())
                        .output()
                        .unwrap();
                    assert!(output.status.success());
                    String::from_utf8(output.stdout).unwrap()
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    // Every command's shell has its own sshd session parent, all on one login.
    assert_eq!(outputs.len(), 6);
    assert_eq!(registry.lock().len(), 1);
    eprintln!("Shared channels ran: {outputs:?}");
}

#[test]
fn only_a_reconnecting_bridge_retires_the_masters() {
    let (registry, fixture) = fake("bridge", READY);
    let first = open(&registry, Lane::Quick).unwrap();
    // The first connect of a bridge keeps the master a startup read opened.
    registry.bridge_connecting("devbox", "default");
    registry.bridge_connecting("devbox", "work");
    assert_eq!(open(&registry, Lane::Quick).unwrap().socket, first.socket);
    // A reconnect may take another network path.
    registry.bridge_connecting("devbox", "default");
    assert_ne!(open(&registry, Lane::Quick).unwrap().socket, first.socket);
    assert_eq!(fixture.starts(), 2);
}

#[test]
fn a_master_that_ignores_sigterm_is_still_ended() {
    // `ssh` keeps SIGTERM ignored when it started with it ignored.
    let (registry, fixture) = fake("ignores-term", &format!("trap '' TERM; {READY}"));
    let channel = open(&registry, Lane::Quick).unwrap();
    let pid = fixture.pid(&channel.socket);
    drop(channel);
    sweep(&mut registry.lock(), Instant::now() + IDLE, IDLE);
    wait_until(|| !alive(&pid));
}

#[test]
fn a_watchdog_that_inherits_ignored_sigterm_leaves_no_watcher_behind() {
    // A GUI application can start with SIGTERM ignored, and the watchdog and
    // its watcher inherit that. Once the master exits by itself, nothing may
    // still hold the pipe: a lingering watcher would signal the master's
    // reaped pid when the pipe later closes.
    let dir = ControlDir::create("herdr-mux-test").unwrap();
    let mut watchdog = Command::new("/bin/sh")
        .args([
            "-c",
            r#"trap '' TERM; exec /bin/sh -c "$0" herdr-ssh-mux "$@""#,
            WATCHDOG,
        ])
        .arg(dir.path())
        .args(["/bin/sh", "-c", "sleep 0.2; exit 3"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut stdin = watchdog.stdin.take().unwrap();
    assert_eq!(watchdog.wait().unwrap().code(), Some(3));
    let written = io::Write::write_all(&mut stdin, b"x");
    assert_eq!(
        written.map_err(|error| error.kind()),
        Err(io::ErrorKind::BrokenPipe)
    );
}

#[test]
fn a_host_with_the_users_own_master_gets_none_of_ours() {
    // Commands connecting directly ride the user's master, which may carry
    // an interactive login this one could not make.
    let (registry, fixture) = fake_with("user-master", "exit 0", READY, IDLE);
    assert!(open(&registry, Lane::Quick).is_none());
    assert_eq!(fixture.starts(), 0);
}

#[test]
fn repeated_failures_back_off_longer_up_to_a_ceiling() {
    assert_eq!(backoff(1), BACKOFF);
    assert_eq!(backoff(2), BACKOFF * 2);
    assert_eq!(backoff(3), BACKOFF * 4);
    assert_eq!(backoff(40), MAX_BACKOFF);
    let (registry, _fixture) = fake("backoff", "exit 255");
    let key = ("devbox".to_owned(), Lane::Quick);
    for failures in 1..=3 {
        let asked = Instant::now();
        assert!(open(&registry, Lane::Quick).is_none());
        let mut hosts = registry.lock();
        let host = hosts.get_mut(&key).unwrap();
        assert_eq!(host.failures, failures);
        let Phase::Direct { until } = host.phase else {
            panic!("not backing off");
        };
        assert!(until >= asked + backoff(failures));
        // Let the next open try again at once.
        host.phase = Phase::Direct {
            until: Instant::now(),
        };
    }
}

#[test]
fn a_master_for_a_skipped_host_reads_no_known_hosts_ahead_of_the_default() {
    let socket = Path::new("/tmp/herdr-mux-1-1/c");
    crate::ssh::set_host_keys("skipped-master.test", crate::ssh::HostKeys::Skip);
    let args = master_args("skipped-master.test", Lane::Quick, socket);
    crate::ssh::set_host_keys("skipped-master.test", crate::ssh::HostKeys::Verify);
    let at = |option: &str| args.iter().position(|arg| arg == option);
    let strict = at("StrictHostKeyChecking=yes").unwrap();
    for option in [
        "StrictHostKeyChecking=no",
        "UserKnownHostsFile=/dev/null",
        "GlobalKnownHostsFile=/dev/null",
    ] {
        assert!(at(option).is_some_and(|at| at < strict), "{option}");
    }
    // A verified host gets none of it.
    let verified = master_args("verified-master.test", Lane::Quick, socket);
    assert!(
        !verified
            .iter()
            .any(|arg| arg == "UserKnownHostsFile=/dev/null")
    );
}
