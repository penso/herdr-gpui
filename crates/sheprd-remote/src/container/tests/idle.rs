use crate::container::idle::{Verdict, verdict};
use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

const AUTO_STOP: Duration = Duration::from_secs(30 * 60);

#[test]
fn a_container_first_seen_idle_starts_its_timeout_rather_than_stopping() {
    let mut seen = HashMap::new();
    let start = Instant::now();
    assert_eq!(
        verdict(&mut seen, "c", false, start, AUTO_STOP),
        Verdict::Keep
    );
    assert_eq!(
        verdict(&mut seen, "c", false, start + AUTO_STOP / 2, AUTO_STOP),
        Verdict::Keep
    );
    assert_eq!(
        verdict(&mut seen, "c", false, start + AUTO_STOP, AUTO_STOP),
        Verdict::Stop
    );
    assert!(seen.is_empty(), "a stopped container starts over");
}

#[test]
fn an_agent_at_work_holds_the_container_off() {
    let mut seen = HashMap::new();
    let start = Instant::now();
    verdict(&mut seen, "c", false, start, AUTO_STOP);
    let later = start + AUTO_STOP - Duration::from_secs(60);
    assert_eq!(
        verdict(&mut seen, "c", true, later, AUTO_STOP),
        Verdict::Keep
    );
    assert_eq!(
        verdict(&mut seen, "c", false, start + AUTO_STOP, AUTO_STOP),
        Verdict::Keep,
        "the timeout restarts from the last work"
    );
    assert_eq!(
        verdict(&mut seen, "c", false, later + AUTO_STOP, AUTO_STOP),
        Verdict::Stop
    );
}

#[test]
fn auto_stop_is_never_or_at_least_five_minutes() {
    use crate::container::settings::auto_stop;
    assert_eq!(auto_stop("never"), Some(None));
    assert_eq!(auto_stop("30m"), Some(Some(Duration::from_secs(1800))));
    assert_eq!(auto_stop("2h"), Some(Some(Duration::from_secs(7200))));
    for bad in ["4m", "0h", "30", "m", "30s", "-5m", ""] {
        assert_eq!(auto_stop(bad), None, "{bad}");
    }
}

#[test]
fn only_a_reply_naming_every_agent_idle_lets_a_container_stop() {
    use crate::container::idle::busy_reply;
    let list = |statuses: &[&str]| {
        let agents = statuses
            .iter()
            .map(|status| {
                format!(r#"{{"agent_status":"{status}","state_labels":{{"working":"x"}}}}"#)
            })
            .collect::<Vec<_>>()
            .join(",");
        format!(r#"{{"id":"cli:agent:list","result":{{"type":"agent_list","agents":[{agents}]}}}}"#)
    };
    assert!(!busy_reply(list(&[]).as_bytes()));
    assert!(
        !busy_reply(list(&["idle", "done"]).as_bytes()),
        "labels do not count"
    );
    for busy in ["working", "blocked", "unknown", "newer"] {
        assert!(busy_reply(list(&["idle", busy]).as_bytes()), "{busy}");
    }
    let down =
        br#"{"id":"cli:agent:list","error":{"code":"server_not_running","message":"no server"}}"#;
    assert!(!busy_reply(down));
    let other = br#"{"id":"cli:agent:list","error":{"code":"internal","message":"x"}}"#;
    assert!(busy_reply(other));
    assert!(busy_reply(b"sh: herdr: not found"));
}
