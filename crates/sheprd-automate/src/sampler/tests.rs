use super::*;
use crate::testing::Fixture;

#[test]
fn only_a_closed_pane_ends_an_agent_not_a_failed_read() {
    let fixture = Fixture::new();
    let store = fixture.store();
    for (position, pane) in [(1, "w1p1"), (2, "gone"), (3, "flaky")] {
        store
            .add_agent(&AgentRow {
                run: 1,
                position,
                path: "script".to_owned(),
                harness: "claude".to_owned(),
                pane: pane.to_owned(),
                cwd: "/tmp".to_owned(),
                started_at: now(),
                ..AgentRow::default()
            })
            .unwrap();
    }
    let home = fixture.root();
    Sampler::default()
        .sample(&store, &fixture.herdr, &home)
        .unwrap();
    let live = store
        .agents(true)
        .unwrap()
        .into_iter()
        .map(|agent| agent.pane)
        .collect::<Vec<_>>();
    assert!(live.contains(&"w1p1".to_owned()), "{live:?}");
    assert!(
        live.contains(&"flaky".to_owned()),
        "a failed read is not an exit: {live:?}"
    );
    assert!(!live.contains(&"gone".to_owned()), "{live:?}");
}
