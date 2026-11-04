use super::*;
use std::fs;

#[cfg(unix)]
fn job<'a>(
    args: &'a [&'a OsStr],
    log: &'a Path,
    timeout: Duration,
    stop: &'a AtomicBool,
) -> Job<'a> {
    Job {
        program: OsStr::new("/bin/sh"),
        args,
        cwd: None,
        env: &[],
        log,
        timeout,
        stop,
    }
}

#[cfg(unix)]
#[test]
fn output_goes_to_the_log_and_its_tail_is_kept() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("runs/1/step-1.log");
    let stop = AtomicBool::new(false);
    let args = [
        OsStr::new("-c"),
        OsStr::new("echo out; echo err >&2; exit 3"),
    ];
    let finished = run(&job(&args, &log, Duration::from_secs(10), &stop)).unwrap();
    assert_eq!(finished.exit, Some(3));
    assert!(!finished.ok());
    assert!(finished.log_tail.contains("out") && finished.log_tail.contains("err"));
    assert!(fs::read_to_string(&log).unwrap().contains("out"));
}

#[cfg(unix)]
#[test]
fn a_timeout_stops_the_whole_group() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("step.log");
    let stop = AtomicBool::new(false);
    // The child starts a grandchild; both must go.
    let args = [OsStr::new("-c"), OsStr::new("sleep 30 & sleep 30")];
    let started = Instant::now();
    let finished = run(&job(&args, &log, Duration::from_millis(300), &stop)).unwrap();
    assert!(finished.timed_out);
    assert!(!finished.ok());
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[cfg(unix)]
#[test]
fn a_stop_request_stops_the_command() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("step.log");
    let stop = AtomicBool::new(true);
    let args = [OsStr::new("-c"), OsStr::new("sleep 30")];
    let finished = run(&job(&args, &log, Duration::from_secs(30), &stop)).unwrap();
    assert!(finished.stopped);
}

#[cfg(unix)]
#[test]
fn a_worker_that_ignores_the_stop_is_killed_after_its_parent_exits() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("step.log");
    let pid = dir.path().join("worker.pid");
    let stop = AtomicBool::new(false);
    // The shell exits on TERM; the worker it started ignores it.
    let script = format!(
        "( trap '' TERM; sleep 30 ) & echo $! > '{}'; wait",
        pid.display()
    );
    let args = [OsStr::new("-c"), OsStr::new(&script)];
    let finished = run(&job(&args, &log, Duration::from_millis(500), &stop)).unwrap();
    assert!(finished.timed_out);
    let worker = fs::read_to_string(&pid)
        .unwrap()
        .trim()
        .parse::<i32>()
        .unwrap();
    let worker = rustix::process::Pid::from_raw(worker).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while rustix::process::test_kill_process(worker).is_ok() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(50));
    }
    assert!(
        rustix::process::test_kill_process(worker).is_err(),
        "the worker outlived the stop"
    );
}

#[test]
fn tails_keep_whole_lines() {
    let dir = tempfile::tempdir().unwrap();
    let log = dir.path().join("log");
    let text = (1..=300)
        .map(|line| format!("line {line}\n"))
        .collect::<String>();
    fs::write(&log, &text).unwrap();
    let (end, truncated) = tail_lines(&log, 64, usize::MAX).unwrap();
    assert!(truncated);
    assert!(end.lines().all(|line| line.starts_with("line ")), "{end}");
    assert!(end.ends_with("line 300"));
    let (end, truncated) = tail(&log, 1_000_000).unwrap();
    assert!(truncated, "only the last {TAIL_LINES} lines are kept");
    assert_eq!(end.lines().count(), TAIL_LINES);
    let (short, truncated) = tail_lines(&log, 1_000_000, usize::MAX).unwrap();
    assert!(!truncated);
    assert_eq!(short.lines().count(), 300);
}
