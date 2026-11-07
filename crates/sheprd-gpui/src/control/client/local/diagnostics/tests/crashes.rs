use super::*;

fn report(dir: &Path, stamp: &str, body: &str) {
    std::fs::write(dir.join(format!("crash-{stamp}-1.txt")), body).unwrap();
}

#[test]
fn the_newest_crash_reports_are_quoted_and_redacted() {
    let dir = tempfile::tempdir().unwrap();
    for (stamp, body) in [
        ("20261001-100000.000", "oldest"),
        ("20261002-100000.000", "older"),
        ("20261003-100000.000", "message: token=\"abc\"\n"),
        (
            "20261004-100000.000",
            "location: /Users/someone/src/app.rs:1:1",
        ),
        ("20261005-100000.000", "newest\n"),
    ] {
        report(dir.path(), stamp, body);
    }
    std::fs::write(dir.path().join("herdr-gpui.jsonl"), "not a report").unwrap();
    let text = redactor().redact(&crash_reports(dir.path()));
    assert_eq!(
        text,
        "-- crash-20261005-100000.000-1.txt --\nnewest\n\
         -- crash-20261004-100000.000-1.txt --\nlocation: ~/src/app.rs:1:1\n\
         -- crash-20261003-100000.000-1.txt --\nmessage: token=[redacted]\n\
         (2 older not included)\n"
    );
}

#[test]
fn no_crash_reports_and_oversized_ones_say_so() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(crash_reports(dir.path()), "(none)\n");
    let big = "x".repeat(MAX_CRASH_REPORT as usize + 1);
    report(dir.path(), "20261005-100000.000", &big);
    let text = crash_reports(dir.path());
    assert!(text.contains("(omitted: larger than"), "{text}");
    assert!(!text.contains("xxxx"));
}

#[cfg(windows)]
#[test]
fn minidumps_are_listed_but_never_included() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("crash-20261005-100000.000-1.dmp"),
        b"MDMP secret memory",
    )
    .unwrap();
    let text = crash_reports(dir.path());
    assert!(
        text.contains("crash-20261005-100000.000-1.dmp (1 KiB)"),
        "{text}"
    );
    assert!(!text.contains("secret"), "{text}");
}
