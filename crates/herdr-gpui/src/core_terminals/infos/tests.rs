use super::*;
use herdr_backend::RunState;
use herdr_client::protocol::AgentStatus;

fn info(name: &str) -> TerminalInfo {
    TerminalInfo {
        agent: AgentStatus::Unknown,
        cwd: None,
        id: TerminalId::new(name).unwrap(),
        labels: Default::default(),
        name: None,
        state: RunState::Running,
        title: None,
    }
}

/// Delivers `list` as the answer to a read already on its way.
fn answer(infos: &mut Infos, list: Vec<TerminalInfo>) -> bool {
    let (send, receive) = sync_channel(1);
    send.send(Ok(list)).unwrap();
    infos.pending = Some(receive);
    infos.last = Some(Instant::now());
    infos.poll(None, Instant::now())
}

#[test]
fn a_started_terminal_is_known_at_once() {
    let mut infos = Infos::default();
    let started = infos.insert(info("t1"));
    assert_eq!(infos.get(&started.id), Some(&started));
}

#[test]
fn a_started_terminal_outlives_one_list_that_missed_it() {
    let mut infos = Infos::default();
    infos.insert(info("t2"));
    // A list asked for before the start names only the older terminal.
    assert!(answer(&mut infos, vec![info("t1")]));
    assert!(infos.get(&TerminalId::new("t2").unwrap()).is_some());
    assert!(infos.get(&TerminalId::new("t1").unwrap()).is_some());
    // The next one is the truth: a terminal it does not name is gone.
    assert!(answer(&mut infos, vec![info("t1")]));
    assert!(infos.get(&TerminalId::new("t2").unwrap()).is_none());
}
