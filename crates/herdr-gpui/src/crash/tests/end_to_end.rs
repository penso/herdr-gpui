//! The installed handlers, in a child process: this test binary run again
//! with only `crashing_child`, which crashes as asked.
use super::*;
use std::process::{Command, Stdio};

const CHILD: &str = "HERDR_GPUI_CRASH_CHILD";
/// An application-defined exception code (bit 29 set), which no system
/// component raises.
#[cfg(windows)]
const TEST_EXCEPTION: u32 = 0xE0DE_AD01;

#[test]
#[ignore = "only crashes when the tests below run it as a child"]
fn crashing_child() {
    let Some(kind) = std::env::var_os(CHILD) else {
        return;
    };
    crate::diagnostics::init().unwrap();
    install();
    match kind.to_str() {
        Some("panic") => panic!("deliberate test panic"),
        #[cfg(windows)]
        Some("exception") => {
            quiet_error_reporting();
            // SAFETY: no arguments; the exception is never handled, which
            // is the point: the process ends in the unhandled-exception filter.
            #[allow(unsafe_code)]
            unsafe {
                windows_sys::Win32::System::Diagnostics::Debug::RaiseException(
                    TEST_EXCEPTION,
                    0,
                    0,
                    std::ptr::null(),
                );
            }
        }
        #[cfg(windows)]
        Some("overflow") => {
            quiet_error_reporting();
            println!("{}", deeper(0));
        }
        _ => {}
    }
}

/// Recurses until the stack runs out; `black_box` keeps each frame.
#[cfg(windows)]
fn deeper(depth: u64) -> u64 {
    if depth == u64::MAX {
        return 0;
    }
    let frame = std::hint::black_box([depth; 64]);
    deeper(depth + 1) + frame[0]
}

/// Windows Error Reporting still runs after the filter; it must not wait on
/// a dialog in a desktop session.
#[cfg(windows)]
fn quiet_error_reporting() {
    use windows_sys::Win32::System::Diagnostics::Debug::{SEM_NOGPFAULTERRORBOX, SetErrorMode};
    // SAFETY: sets a process flag; no pointers.
    #[allow(unsafe_code)]
    unsafe {
        SetErrorMode(SEM_NOGPFAULTERRORBOX);
    }
}

/// Runs `crashing_child` with `kind`; returns its logs directory.
fn crash(kind: &str, state: &Path) -> PathBuf {
    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "crash::tests::end_to_end::crashing_child",
            "--ignored",
            "--test-threads=1",
        ])
        .env(CHILD, kind)
        .env("XDG_STATE_HOME", state)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(!status.success(), "the child did not crash");
    state.join("herdr").join("gpui").join("logs")
}

fn only(dir: &Path, extension: &str) -> PathBuf {
    let found = files(dir, extension);
    assert_eq!(found.len(), 1, "{found:?}");
    found.into_iter().next().unwrap()
}

#[test]
fn a_panic_leaves_a_report_and_a_flushed_log_record() {
    let state = tempfile::tempdir().unwrap();
    let logs = crash("panic", state.path());
    let report_path = only(&logs, REPORT_EXTENSION);
    let report = fs::read_to_string(&report_path).unwrap();
    assert!(
        report.contains("message: deliberate test panic\n"),
        "{report}"
    );
    assert!(report.contains("end_to_end.rs"), "{report}");
    assert!(report.contains("\nbacktrace:\n"), "{report}");
    let log = fs::read_to_string(logs.join("herdr-gpui.jsonl")).unwrap();
    let name = report_path.file_name().unwrap().to_str().unwrap();
    let record = log
        .lines()
        .find(|line| line.contains("Herdr UI panicked"))
        .unwrap_or_else(|| panic!("no panic record in\n{log}"));
    assert!(record.contains(name) && record.contains("deliberate test panic"));
    #[cfg(windows)]
    assert_eq!(
        &fs::read(only(&logs, DUMP_EXTENSION)).unwrap()[..4],
        b"MDMP"
    );
}

#[cfg(windows)]
#[test]
fn an_unhandled_exception_leaves_a_dump_and_a_report() {
    let state = tempfile::tempdir().unwrap();
    let logs = crash("exception", state.path());
    assert_eq!(
        &fs::read(only(&logs, DUMP_EXTENSION)).unwrap()[..4],
        b"MDMP"
    );
    let report = fs::read_to_string(only(&logs, REPORT_EXTENSION)).unwrap();
    assert!(
        report.contains(&format!("unhandled exception {TEST_EXCEPTION:#010x}")),
        "{report}"
    );
    let log = fs::read_to_string(logs.join("herdr-gpui.jsonl")).unwrap();
    assert!(log.contains("Herdr UI crashed"), "{log}");
}

#[cfg(windows)]
#[test]
fn a_stack_overflow_still_leaves_a_dump() {
    let state = tempfile::tempdir().unwrap();
    let logs = crash("overflow", state.path());
    assert_eq!(
        &fs::read(only(&logs, DUMP_EXTENSION)).unwrap()[..4],
        b"MDMP"
    );
    let report = fs::read_to_string(only(&logs, REPORT_EXTENSION)).unwrap();
    // STATUS_STACK_OVERFLOW
    assert!(
        report.contains("unhandled exception 0xc00000fd"),
        "{report}"
    );
}
