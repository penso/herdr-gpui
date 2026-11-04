use super::*;

#[test]
fn wanted_agents_wait_for_the_lookup_and_name_what_is_missing() {
    let mut fan_out = idle();
    fan_out.stage = Stage::Compose(None);
    fan_out.want(vec![(AgentKind::Claude, 2), (AgentKind::Kimi, 1)]);
    assert_eq!(fan_out.picks.total(), 0, "nothing is known yet");
    fan_out
        .sender
        .send(Event::Installed(Ok(vec![
            AgentKind::Claude,
            AgentKind::Codex,
        ])))
        .unwrap();
    fan_out.poll();
    assert_eq!(fan_out.picks.count(AgentKind::Claude), 2);
    assert_eq!(fan_out.picks.total(), 2);
    assert_eq!(
        fan_out
            .error
            .as_deref()
            .map(|error| error.replace(['\u{2068}', '\u{2069}'], ""))
            .as_deref(),
        Some("Not installed on This Mac: kimi")
    );
    assert!(fan_out.composing(), "picking never launches");
    // Applied once: a later lookup does not pick them again.
    fan_out
        .sender
        .send(Event::Installed(Ok(vec![AgentKind::Claude])))
        .unwrap();
    fan_out.poll();
    assert_eq!(fan_out.picks.total(), 2);
}

#[test]
fn wanted_agents_stop_at_the_lane_limit() {
    let mut fan_out = idle();
    fan_out.want(vec![(AgentKind::Claude, 5), (AgentKind::Codex, 5)]);
    assert_eq!(fan_out.picks.total(), MAX_LANES_FOR_TEST);
}

const MAX_LANES_FOR_TEST: usize = crate::fan_out::MAX_LANES;
