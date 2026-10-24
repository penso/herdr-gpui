use super::*;

#[test]
fn last_seen_reads_in_the_largest_whole_unit() {
    let now = 100 * 86_400_000;
    for (ago_ms, text) in [
        (0, "just now"),
        (59_000, "just now"),
        (60_000, "1 min ago"),
        (59 * 60_000, "59 min ago"),
        (3_600_000, "1 h ago"),
        (23 * 3_600_000, "23 h ago"),
        (86_400_000, "yesterday"),
        (3 * 86_400_000, "3 days ago"),
    ] {
        assert_eq!(ago(now - ago_ms, now), text, "{ago_ms}");
    }
    // A clock that moved back reads as now rather than in the future.
    assert_eq!(ago(now + 5_000, now), "just now");
}

#[test]
fn the_daemon_keeps_its_state_beside_the_app_s() {
    if let (Ok(dir), Some(app)) = (state_dir(), crate::preferences::state_dir()) {
        assert_eq!(dir, app.join("remote"));
    }
}
