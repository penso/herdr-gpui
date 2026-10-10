use crate::devices_overview::{Device, Link, Tally, chart::padded, history::STEPS};
use herdr_client::protocol::AgentStatus::{self, Blocked, Done, Idle, Unknown, Working};

#[test]
fn a_tally_counts_unknown_agents_as_idle() {
    let tally = Tally::of([Working, Working, Blocked, Done, Idle, Unknown]);
    assert_eq!(
        tally,
        Tally {
            working: 2,
            blocked: 1,
            done: 1,
            idle: 2,
        }
    );
    assert_eq!(tally.total(), 6);
    let counts = tally.counts();
    assert_eq!((counts.working, counts.blocked), (2, 1));
}

#[test]
fn devices_sum_into_the_totals() {
    let devices = [
        Device::for_test("mac", Link::Online, &[Working, Idle]),
        Device::for_test("studio", Link::Online, &[Working, Blocked, Done]),
    ];
    assert_eq!(
        Tally::sum(&devices),
        Tally {
            working: 2,
            blocked: 1,
            done: 1,
            idle: 1,
        }
    );
}

#[test]
fn dots_put_working_agents_first_and_stop_at_eight() {
    let statuses: Vec<AgentStatus> = [Idle, Blocked, Working, Done]
        .into_iter()
        .cycle()
        .take(12)
        .collect();
    let device = Device::for_test("studio", Link::Online, &statuses);
    assert_eq!(device.dots.len(), 8);
    assert_eq!(&device.dots[..3], &[Working, Working, Working]);
    assert_eq!(device.tally.total(), 12, "the count still says them all");
}

#[test]
fn search_matches_every_word_against_device_agent_and_workspace() {
    let device = Device::for_test("mac-studio", Link::Online, &[Working]);
    assert!(device.matches(""));
    assert!(device.matches("  "));
    assert!(device.matches("STUDIO"));
    assert!(device.matches("ssh"), "how it is reached");
    assert!(device.matches("claude gpui"), "agent and workspace");
    assert!(!device.matches("claude laptop"));
}

#[test]
fn the_chart_axis_always_spans_two_hours() {
    let short = padded(vec![1, 2, 3]);
    assert_eq!(short.len(), STEPS + 1);
    assert!(short[..STEPS - 2].iter().all(Option::is_none));
    assert_eq!(short[STEPS - 2..], [Some(1), Some(2), Some(3)]);
    let long = padded((0..STEPS + 5).collect());
    assert_eq!(long.len(), STEPS + 1);
    assert_eq!(long.last(), Some(&Some(STEPS + 4)), "the newest steps stay");
}

#[test]
fn only_a_reachable_device_is_counted() {
    use crate::{devices_overview::online_tally, state::ConnectionStatus};
    let endpoint = |enabled| {
        crate::endpoint::Endpoint::new(
            "ssh:box".into(),
            "box".into(),
            herdr_client::ConnectTarget::Ssh {
                target: "box".into(),
                session: "default".into(),
            },
            enabled,
        )
    };
    let mut live = crate::state::LiveState::default();
    live.snapshot = Some(std::sync::Arc::new(crate::sidebar::layout_tests::snapshot(
        1,
    )));
    assert_eq!(online_tally(&endpoint(true), &live), None, "not connected");
    live.status = ConnectionStatus::Connected;
    let tally = online_tally(&endpoint(true), &live).unwrap();
    assert_eq!(tally.total(), live.snapshot.as_ref().unwrap().agents.len());
    assert_eq!(online_tally(&endpoint(false), &live), None, "disabled");
    live.snapshot = None;
    assert_eq!(
        online_tally(&endpoint(true), &live),
        None,
        "no snapshot yet"
    );
}
