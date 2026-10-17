#![allow(clippy::unwrap_used)]
use super::*;
use chrono::TimeZone as _;

mod end_to_end;

fn at(seconds: i64, millis: u32) -> chrono::DateTime<chrono::Local> {
    chrono::Local
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
        "2026-10-17 23:05:12.042 +02:00",
        "main",
        "boom",
        "src/app.rs:1:2",
        "   0: herdr_gpui::app::run",
    );
    for expected in [
        "Herdr UI crash report\n",
        &format!("version: {}\n", crate::constants::APP_VERSION),
        "thread: main\n",
        "location: src/app.rs:1:2\n",
        "message: boom\n",
        "\nbacktrace:\n   0: herdr_gpui::app::run\n",
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
