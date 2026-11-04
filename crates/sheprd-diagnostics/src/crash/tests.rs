#![allow(clippy::unwrap_used)]
use super::*;
use chrono::TimeZone as _;

mod end_to_end;

fn at(seconds: i64, millis: u32) -> chrono::DateTime<chrono::Utc> {
    chrono::Utc
        .timestamp_opt(1_790_000_000 + seconds, millis * 1_000_000)
        .unwrap()
}

#[test]
fn names_sort_by_time_and_carry_the_process() {
    let early = file_name(at(0, 5), 4242, REPORT_EXTENSION);
    let later = file_name(at(0, 40), 7, REPORT_EXTENSION);
    let next = file_name(at(1, 0), 1, REPORT_EXTENSION);
    assert!(
        early.starts_with(PREFIX) && early.ends_with("-4242.txt"),
        "{early}"
    );
    assert!(early < later && later < next, "{early} {later} {next}");
}

#[test]
fn payloads_read_as_text() {
    let literal: Box<dyn std::any::Any + Send> = Box::new("index out of range");
    assert_eq!(message(literal.as_ref()), "index out of range");
    let formatted: Box<dyn std::any::Any + Send> = Box::new(format!("bad {}", 3));
    assert_eq!(message(formatted.as_ref()), "bad 3");
    let other: Box<dyn std::any::Any + Send> = Box::new(3_u8);
    assert_eq!(message(other.as_ref()), "Box<dyn Any>");
}

#[test]
fn a_report_names_the_build_place_and_backtrace() {
    let text = report(
        "1.2.3",
        "2026-10-17 23:05:12.042 +02:00",
        "main",
        "boom",
        "src/app.rs:1:2",
        "   0: sheprd_gpui::app::run",
    );
    for expected in [
        "Sheprd crash report\n",
        "version: 1.2.3\n",
        "thread: main\n",
        "location: src/app.rs:1:2\n",
        "message: boom\n",
        "\nbacktrace:\n   0: sheprd_gpui::app::run\n",
    ] {
        assert!(text.contains(expected), "{expected:?} missing from\n{text}");
    }
}

#[test]
fn only_the_newest_reports_are_kept_and_nothing_else_is_touched() {
    let dir = tempfile::tempdir().unwrap();
    let names: Vec<_> = (0..KEEP_REPORTS + 2)
        .map(|second| file_name(at(second as i64, 0), 1, REPORT_EXTENSION))
        .collect();
    for name in &names {
        write_new(&dir.path().join(name), "report").unwrap();
    }
    let unrelated = ["herdr-gpui.jsonl", "crash-notes.md", "other-20200101.txt"];
    for name in unrelated {
        fs::write(dir.path().join(name), "keep").unwrap();
    }
    prune(dir.path(), REPORT_EXTENSION, KEEP_REPORTS);
    let kept: Vec<_> = files(dir.path(), REPORT_EXTENSION)
        .iter()
        .map(|path| path.file_name().unwrap().to_str().unwrap().to_owned())
        .collect();
    let mut newest: Vec<_> = names.iter().rev().take(KEEP_REPORTS).cloned().collect();
    newest.sort_by(|a, b| b.cmp(a));
    assert_eq!(kept, newest);
    for name in unrelated {
        assert!(dir.path().join(name).is_file(), "{name} was removed");
    }
}

#[test]
fn a_report_never_replaces_an_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("crash-1.txt");
    write_new(&path, "first").unwrap();
    let error = write_new(&path, "second").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read_to_string(&path).unwrap(), "first");
}

#[cfg(windows)]
#[test]
fn this_process_can_write_a_minidump() {
    let dir = tempfile::tempdir().unwrap();
    let path = windows::write_dump(dir.path(), None).unwrap();
    assert_eq!(path.extension().unwrap(), DUMP_EXTENSION);
    let bytes = fs::read(&path).unwrap();
    // Every minidump starts with this signature.
    assert_eq!(&bytes[..4], b"MDMP");
}

#[test]
fn names_are_utc_so_a_clock_moving_back_keeps_their_order() {
    // 2026-10-25 01:30 UTC is 02:30 CET, after Central Europe's clocks went
    // back from 03:00 CEST; local time would sort it before 02:45 CEST.
    let before = chrono::Utc
        .with_ymd_and_hms(2026, 10, 25, 0, 45, 0)
        .unwrap();
    let after = chrono::Utc
        .with_ymd_and_hms(2026, 10, 25, 1, 30, 0)
        .unwrap();
    assert_eq!(
        file_name(before, 9, REPORT_EXTENSION),
        "crash-20261025-004500.000-9.txt"
    );
    assert!(file_name(before, 9, REPORT_EXTENSION) < file_name(after, 9, REPORT_EXTENSION));
}

#[test]
fn only_names_the_app_writes_count_as_crash_files() {
    let ours = file_name(at(0, 42), 4242, REPORT_EXTENSION);
    assert!(is_crash_name(&ours, REPORT_EXTENSION));
    assert!(!is_crash_name(&ours, "dmp"));
    for name in [
        "crash-20200101-notes.txt",
        "crash-notes.txt",
        "crash-20261017-210512.042-.txt",
        "crash-20261017-210512.042-12a.txt",
        "crash-20261317-210512.042-1.txt",
        "crash-2026117-210512.042-1.txt",
        "crash-20261017-210512-1.txt",
        "crash-20261017-210512.042-1.txt.bak",
    ] {
        assert!(!is_crash_name(name, REPORT_EXTENSION), "{name}");
    }
    let dir = tempfile::tempdir().unwrap();
    for name in [ours.as_str(), "crash-20200101-notes.txt"] {
        fs::write(dir.path().join(name), "x").unwrap();
    }
    prune(dir.path(), REPORT_EXTENSION, 0);
    assert!(!dir.path().join(&ours).exists());
    assert!(dir.path().join("crash-20200101-notes.txt").is_file());
}
