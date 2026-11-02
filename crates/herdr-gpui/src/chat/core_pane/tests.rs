use super::*;
use herdr_backend::RunState;
use herdr_client::protocol::AgentStatus;

const A: &str = "0123456789abcdef0123456789abcdef";
const B: &str = "fedcba9876543210fedcba9876543210";

fn info(id: &str, endpoint: Option<&str>) -> TerminalInfo {
    TerminalInfo {
        agent: AgentStatus::Unknown,
        cwd: None,
        id: TerminalId::new(id).unwrap(),
        labels: endpoint
            .map(|endpoint| BTreeMap::from([(TOKEN.to_owned(), endpoint.to_owned())]))
            .unwrap_or_default(),
        name: None,
        state: RunState::Running,
        title: None,
    }
}

fn herdr(pane: &str) -> Target {
    Target {
        endpoint: A.parse().unwrap(),
        existing: None,
        pane_id: pane.into(),
        route: Route::Local,
        surface: Surface::Herdr,
    }
}

#[test]
fn a_labelled_terminal_is_a_chat_drawn_over_its_pane() {
    let terminals = [info("t1", Some(A)), info("t2", None)];
    let targets = targets(&terminals);
    assert_eq!(targets.len(), 1);
    let target = &targets[0];
    assert_eq!(target.endpoint.as_str(), A);
    assert_eq!(target.pane_id, "core:t1");
    assert_eq!(target.route, Route::Local);
    assert_eq!(
        target.surface,
        Surface::Core(TerminalId::new("t1").unwrap())
    );
    assert!(target.existing.is_none());
}

#[test]
fn invalid_labels_and_other_machines_terminals_are_left_out() {
    let terminals = [
        info("t1", Some("not-an-endpoint")),
        info("t2", Some("../../etc/passwd")),
        info("box/t3", Some(A)),
        info("t4", Some(B)),
    ];
    let targets = targets(&terminals);
    let keys: Vec<&str> = targets
        .iter()
        .map(|target| target.pane_id.as_str())
        .collect();
    assert_eq!(keys, ["core:t4"]);
}

#[test]
fn a_terminal_shown_twice_is_one_chat_and_the_count_is_bounded() {
    let mut terminals = vec![info("t0", Some(A)), info("t0", Some(A))];
    terminals.extend((1..=MAX_CHATS + 4).map(|n| info(&format!("t{n}"), Some(B))));
    let targets = targets(&terminals);
    assert_eq!(targets.len(), MAX_CHATS);
    assert_eq!(targets[0].pane_id, "core:t0");
    assert_eq!(targets[1].pane_id, "core:t1");
}

#[test]
fn herdr_chats_come_first_and_keep_a_clashing_key() {
    let core = targets(&[info("t1", Some(B)), info("t2", Some(B))]);
    let merged = with_core(vec![herdr("w1:p1"), herdr("core:t1")], core);
    let keys: Vec<(&str, &Surface)> = merged
        .iter()
        .map(|target| (target.pane_id.as_str(), &target.surface))
        .collect();
    assert_eq!(
        keys,
        [
            ("w1:p1", &Surface::Herdr),
            ("core:t1", &Surface::Herdr),
            ("core:t2", &Surface::Core(TerminalId::new("t2").unwrap())),
        ]
    );
}

#[test]
fn herdr_chats_fill_the_bound_before_core_ones() {
    let herdr_chats: Vec<Target> = (0..MAX_CHATS).map(|n| herdr(&format!("w1:p{n}"))).collect();
    let merged = with_core(herdr_chats, targets(&[info("t1", Some(B))]));
    assert_eq!(merged.len(), MAX_CHATS);
    assert!(merged.iter().all(|target| target.surface == Surface::Herdr));
}

#[test]
fn a_chat_terminal_runs_the_program_with_its_endpoint_and_label() {
    let endpoint: EndpointId = A.parse().unwrap();
    let extra = BTreeMap::from([
        ("FOO".to_owned(), "bar".to_owned()),
        // `[chat.env]` cannot point the program at another endpoint.
        (ENDPOINT_ENV.to_owned(), B.to_owned()),
        (PANE_ENV.to_owned(), "w1:p1".to_owned()),
    ]);
    let spawn = spawn(Agent::Codex, Path::new("/opt/herdr-acp"), &endpoint, &extra);
    assert_eq!(spawn.argv, ["/opt/herdr-acp", "codex"]);
    assert_eq!(
        spawn.env,
        [
            ("FOO".to_owned(), "bar".to_owned()),
            (ENDPOINT_ENV.to_owned(), A.to_owned()),
            (PANE_ENV.to_owned(), String::new()),
        ]
    );
    assert_eq!(
        spawn.labels,
        BTreeMap::from([(TOKEN.to_owned(), A.to_owned())])
    );
    // Two chats with one agent cannot share a herdr-core name.
    assert!(spawn.name.is_none());
    // The split it opens beside fills in the folder.
    assert!(spawn.cwd.is_none());
}

#[test]
fn the_terminal_a_chat_is_started_in_is_found_as_that_chat() {
    let endpoint: EndpointId = A.parse().unwrap();
    let spawn = spawn(
        Agent::Claude,
        Path::new("herdr-acp"),
        &endpoint,
        &BTreeMap::new(),
    );
    let mut started = info("t9", None);
    started.labels = spawn.labels;
    let found = targets(&[started]);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].endpoint, endpoint);
}
