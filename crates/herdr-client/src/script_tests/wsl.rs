#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;

const LIMITS: ScriptLimits = ScriptLimits {
    output: 1 << 20,
    idle: Duration::from_secs(30),
};

fn wsl(distro: &str, script: &str, input: &[u8], cancelled: bool) -> (Result<u64>, Vec<u8>) {
    let mut output = Vec::new();
    let result = run_script(
        ScriptHost::Wsl(distro),
        script,
        input,
        &mut output,
        LIMITS,
        &AtomicBool::new(cancelled),
    );
    (result, output)
}

#[test]
fn a_name_that_is_not_a_distribution_is_refused_first() {
    for name in ["", "-d", "two words", "a;b"] {
        let (result, _) = wsl(name, "true", b"", false);
        assert!(
            matches!(result, Err(Error::Wsl(wsl::Error::InvalidDistro))),
            "{name}"
        );
    }
}

#[cfg(not(windows))]
#[test]
fn a_distribution_needs_a_windows_client() {
    let (result, _) = wsl("Ubuntu", "true", b"", false);
    assert!(matches!(result, Err(Error::Wsl(wsl::Error::Unsupported))));
}

#[cfg(windows)]
#[test]
fn a_windows_client_runs_scripts_only_on_remote_hosts() {
    // This machine has no `/bin/sh`.
    let result = run_script(
        ScriptHost::Local,
        "true",
        std::io::empty(),
        std::io::sink(),
        LIMITS,
        &AtomicBool::new(false),
    );
    assert!(matches!(result, Err(Error::ScriptUnsupported)));
    // An SSH host is accepted: cancelled first, it reports the cancellation
    // instead of the platform, and never starts `ssh`.
    let result = run_script(
        ScriptHost::Ssh("me@box", SshLane::Quick),
        "true",
        std::io::empty(),
        std::io::sink(),
        LIMITS,
        &AtomicBool::new(true),
    );
    assert!(matches!(result, Err(Error::ScriptCancelled)));
}

#[cfg(windows)]
#[test]
fn cancellation_before_start_never_starts_wsl() {
    let (result, _) = wsl("Ubuntu", "true", b"", true);
    assert!(matches!(result, Err(Error::ScriptCancelled)));
}

#[cfg(windows)]
#[test]
fn a_distribution_script_is_one_argument_after_exec() {
    let script = "printf '%s\\n' \"$HOME\" `x` \"q\"";
    let command = wsl::command("Ubuntu-24.04", script);
    let args: Vec<_> = command.get_args().collect();
    assert_eq!(
        args,
        [
            "--distribution",
            "Ubuntu-24.04",
            "--cd",
            "~",
            "--exec",
            "/bin/sh",
            "-c",
            script
        ]
    );
}

/// Runs scripts in a real distribution:
/// `HERDR_TEST_WSL_DISTRO=Ubuntu-22.04 cargo test -p herdr-client
/// live_wsl_scripts -- --ignored --nocapture`.
#[cfg(windows)]
#[test]
#[ignore = "runs scripts in a real WSL distribution"]
fn live_wsl_scripts() {
    let distro = std::env::var("HERDR_TEST_WSL_DISTRO").expect("HERDR_TEST_WSL_DISTRO");
    // Quotes, `$`, backquotes, and newlines reach the distribution's shell as
    // written, and binary input streams through unchanged.
    let input: Vec<u8> = (0..=255).cycle().take(200_000).collect();
    let (result, output) = wsl(
        &distro,
        "cat\nprintf '%s' \"$PWD\" '`$x`' \"q'\"",
        &input,
        false,
    );
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(&output[..input.len()], input.as_slice());
    let tail = String::from_utf8(output[input.len()..].to_vec()).unwrap();
    assert!(tail.ends_with("`$x`q'"), "{tail}");
    assert!(tail.starts_with('/'), "{tail}");

    let (result, _) = wsl(&distro, "printf 'failure\\033' >&2; exit 7", b"", false);
    assert!(
        matches!(&result, Err(Error::ScriptExit { status, stderr }) if status.code() == Some(7) && stderr == "failure"),
        "{result:?}"
    );

    let mut output = Vec::new();
    let result = run_script(
        ScriptHost::Wsl(&distro),
        "head -c 4096 /dev/zero",
        std::io::empty(),
        &mut output,
        ScriptLimits {
            output: 1024,
            idle: Duration::from_secs(30),
        },
        &AtomicBool::new(false),
    );
    assert!(
        matches!(result, Err(Error::ScriptOutputLimit)),
        "{result:?}"
    );
}

/// Runs `script` in `distro` and returns how it ended and how long that took,
/// failing if it outlives `deadline`: a hung pipe reader would never return.
#[cfg(windows)]
fn bounded(
    distro: &str,
    script: &str,
    limits: ScriptLimits,
    cancel_after: Option<Duration>,
    deadline: Duration,
) -> (Result<u64>, Duration) {
    let (done, finished) = std::sync::mpsc::channel();
    let distro = distro.to_owned();
    let script = script.to_owned();
    std::thread::spawn(move || {
        let cancelled = std::sync::Arc::new(AtomicBool::new(false));
        if let Some(after) = cancel_after {
            let flag = cancelled.clone();
            std::thread::spawn(move || {
                std::thread::sleep(after);
                flag.store(true, std::sync::atomic::Ordering::Release);
            });
        }
        let started = std::time::Instant::now();
        let result = run_script(
            ScriptHost::Wsl(&distro),
            &script,
            std::io::empty(),
            std::io::sink(),
            limits,
            &cancelled,
        );
        let _ = done.send((result, started.elapsed()));
    });
    finished
        .recv_timeout(deadline)
        .expect("the script outlived its deadline")
}

/// A silent script and a cancelled one both end promptly on Windows, where
/// only `wsl.exe` is killed, even while a background process inside the
/// distribution still holds the script's output open:
/// `HERDR_TEST_WSL_DISTRO=Ubuntu-22.04 cargo test -p herdr-client
/// live_wsl_stops -- --ignored --nocapture`.
#[cfg(windows)]
#[test]
#[ignore = "runs scripts in a real WSL distribution"]
fn live_wsl_stops() {
    let distro = std::env::var("HERDR_TEST_WSL_DISTRO").expect("HERDR_TEST_WSL_DISTRO");
    let holder = "sleep 57 & sleep 58";
    let quiet = ScriptLimits {
        output: 1024,
        idle: Duration::from_secs(3),
    };
    let (result, took) = bounded(&distro, holder, quiet, None, Duration::from_secs(20));
    println!("idle: {result:?} after {took:?}");
    assert!(matches!(result, Err(Error::ScriptTimeout)), "{result:?}");

    let patient = ScriptLimits {
        output: 1024,
        idle: Duration::from_secs(60),
    };
    let (result, took) = bounded(
        &distro,
        holder,
        patient,
        Some(Duration::from_secs(2)),
        Duration::from_secs(20),
    );
    println!("cancelled: {result:?} after {took:?}");
    assert!(matches!(result, Err(Error::ScriptCancelled)), "{result:?}");
    assert!(took < Duration::from_secs(15), "{took:?}");
}
