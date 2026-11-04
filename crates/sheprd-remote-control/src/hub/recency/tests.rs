use super::*;

fn seen(times: &mut AgentTimes, agents: &[(&str, u64)], now_ms: u64) {
    times.observe("boot-1", agents.iter().copied(), now_ms);
}

#[test]
fn agents_already_there_have_no_known_time() {
    let mut times = AgentTimes::default();
    seen(&mut times, &[("p1", 7)], 100);
    assert_eq!(times.changed_ms("p1"), None);
}

#[test]
fn a_moved_sequence_is_dated_and_keeps_that_time() {
    let mut times = AgentTimes::default();
    seen(&mut times, &[("p1", 7)], 100);
    seen(&mut times, &[("p1", 8)], 200);
    assert_eq!(times.changed_ms("p1"), Some(200));
    seen(&mut times, &[("p1", 8)], 300);
    assert_eq!(times.changed_ms("p1"), Some(200));
}

#[test]
fn an_agent_that_appears_later_just_started() {
    let mut times = AgentTimes::default();
    seen(&mut times, &[("p1", 7)], 100);
    seen(&mut times, &[("p1", 7), ("p2", 1)], 200);
    assert_eq!(times.changed_ms("p2"), Some(200));
}

#[test]
fn a_new_boot_does_not_read_as_every_agent_changing() {
    let mut times = AgentTimes::default();
    seen(&mut times, &[("p1", 7)], 100);
    seen(&mut times, &[("p1", 8)], 200);
    times.observe("boot-2", [("p1", 1)], 300);
    assert_eq!(times.changed_ms("p1"), None);
}

#[test]
fn agents_that_left_are_forgotten() {
    let mut times = AgentTimes::default();
    seen(&mut times, &[("p1", 7), ("p2", 7)], 100);
    seen(&mut times, &[("p1", 8), ("p2", 8)], 200);
    seen(&mut times, &[("p1", 8)], 300);
    assert_eq!(times.panes.len(), 1);
    assert_eq!(times.changed_ms("p2"), None);
}
