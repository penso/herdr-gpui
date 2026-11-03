use super::*;

const ENDPOINT: &str = "0123456789abcdef0123456789abcdef";

fn target() -> Target {
    Target {
        pane: "w1:p2".to_owned(),
        socket: PathBuf::from("/home/me/.config/herdr/herdr.sock"),
    }
}

fn builtin(args: Vec<std::ffi::OsString>) -> AgentChoice {
    AgentChoice::Builtin {
        agent: Agent::Codex,
        args,
    }
}

#[test]
fn a_chat_keeps_its_latest_session_until_it_ends() {
    let dir = tempfile::tempdir().unwrap();
    let endpoint: EndpointId = ENDPOINT.parse().unwrap();
    let keeper = Keeper::new(
        Some(dir.path().to_owned()),
        Some(&target()),
        Some(&endpoint),
        &builtin(Vec::new()),
        true,
        Path::new("/work"),
    )
    .unwrap();
    keeper.opened("first");
    keeper.opened("second");
    let records = restore::list(dir.path());
    assert_eq!(records.len(), 1);
    let record = &records[0].0;
    assert_eq!(record.session, "second");
    assert_eq!(record.agent, Agent::Codex);
    assert!(record.adapter);
    assert_eq!(record.pane, "w1:p2");
    assert_eq!(record.cwd, Path::new("/work"));

    // An id that could not be typed back drops the record rather than
    // leaving the older session to be continued.
    keeper.opened("not safe; rm -rf");
    assert!(restore::list(dir.path()).is_empty());

    keeper.opened("third");
    keeper.ended();
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[test]
fn only_a_plain_builtin_chat_in_a_listening_herdr_pane_is_kept() {
    let endpoint: EndpointId = ENDPOINT.parse().unwrap();
    let dir = Some(PathBuf::from("/state"));
    let cwd = Path::new("/work");
    let keep = |dir: Option<PathBuf>, target: Option<&Target>, endpoint, agent: &AgentChoice| {
        Keeper::new(dir, target, endpoint, agent, false, cwd).is_some()
    };
    assert!(keep(
        dir.clone(),
        Some(&target()),
        Some(&endpoint),
        &builtin(Vec::new())
    ));
    assert!(!keep(
        None,
        Some(&target()),
        Some(&endpoint),
        &builtin(Vec::new())
    ));
    assert!(!keep(
        dir.clone(),
        None,
        Some(&endpoint),
        &builtin(Vec::new())
    ));
    assert!(!keep(
        dir.clone(),
        Some(&target()),
        None,
        &builtin(Vec::new())
    ));
    assert!(!keep(
        dir.clone(),
        Some(&target()),
        Some(&endpoint),
        &builtin(vec!["--model".into()])
    ));
    let command = AgentChoice::Command {
        program: "my-agent".into(),
        args: Vec::new(),
    };
    assert!(!keep(dir, Some(&target()), Some(&endpoint), &command));
}
