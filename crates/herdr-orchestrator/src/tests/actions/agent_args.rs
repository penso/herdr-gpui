use super::*;
use crate::actions::{MAX_ARG_BYTES, MAX_ARGS, valid_args};

/// Each `agent start` call's arguments, exactly as the fake received them.
fn starts(dir: &Path) -> Vec<Vec<String>> {
    std::fs::read_to_string(dir.join("home/argv"))
        .unwrap_or_default()
        .lines()
        .map(|line| {
            line.split_terminator('\x1f')
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|argv| argv.first().map(String::as_str) == Some("agent") && argv[1] == "start")
        .collect()
}

#[test]
fn arguments_follow_the_model_one_word_each() {
    let dir = tempfile::tempdir().unwrap();
    let site = site(dir.path(), "");
    let mut with_args = request();
    with_args.kind = AgentKind::Opencode;
    with_args.model = Some("anthropic/claude-sonnet-5".into());
    with_args.args = vec![
        "--standalone".into(),
        "--agent".into(),
        "review".into(),
        // A shell would split this and expand the variable; Herdr gets it whole.
        "two words $HOME".into(),
    ];
    perform(
        &site,
        &Action::Dispatch(Box::new(with_args)),
        &AtomicBool::new(false),
    )
    .unwrap();
    let start = starts(dir.path()).remove(0);
    let tail = start.iter().position(|arg| arg == "--").unwrap();
    assert_eq!(
        &start[tail..],
        [
            "--",
            "--model",
            "anthropic/claude-sonnet-5",
            "--standalone",
            "--agent",
            "review",
            "two words $HOME"
        ]
    );
}

#[test]
fn arguments_without_a_model_still_come_after_the_separator() {
    let dir = tempfile::tempdir().unwrap();
    let site = site(dir.path(), "");
    let mut with_args = request();
    with_args.args = vec!["--permission-mode".into(), "plan".into()];
    perform(
        &site,
        &Action::Dispatch(Box::new(with_args)),
        &AtomicBool::new(false),
    )
    .unwrap();
    let start = starts(dir.path()).remove(0);
    assert_eq!(
        &start[start.len() - 3..],
        ["--", "--permission-mode", "plan"]
    );
}

#[test]
fn no_arguments_and_no_model_add_no_separator() {
    let dir = tempfile::tempdir().unwrap();
    let site = site(dir.path(), "");
    perform(
        &site,
        &Action::Dispatch(Box::new(request())),
        &AtomicBool::new(false),
    )
    .unwrap();
    let start = starts(dir.path()).remove(0);
    assert!(!start.iter().any(|arg| arg == "--"), "{start:?}");
    assert_eq!(start.last().map(String::as_str), Some("90000"));
}

#[test]
fn arguments_herdr_would_refuse_stop_the_dispatch_before_anything_runs() {
    let dir = tempfile::tempdir().unwrap();
    let site = site(dir.path(), "");
    let mut bad = request();
    bad.args = vec!["--agent".into(), "review\nrm -rf ~".into()];
    let error = perform(
        &site,
        &Action::Dispatch(Box::new(bad)),
        &AtomicBool::new(false),
    )
    .unwrap_err();
    assert!(matches!(error, Error::AgentArguments), "{error}");
    assert!(calls(dir.path()).is_empty(), "nothing ran on the host");
    assert!(
        !site.database.exists()
            || Store::open(&site.database)
                .unwrap()
                .runs()
                .unwrap()
                .is_empty(),
        "no run was recorded"
    );
}

#[test]
fn only_bounded_arguments_without_controls_are_valid() {
    let empty: [&str; 0] = [];
    assert!(valid_args(&empty));
    assert!(valid_args(&[
        "--agent",
        "review",
        "",
        "-c",
        "key=value with spaces"
    ]));
    assert!(valid_args(&vec!["-v"; MAX_ARGS]));
    assert!(valid_args(&["x".repeat(MAX_ARG_BYTES)]));
    assert!(!valid_args(&vec!["-v"; MAX_ARGS + 1]));
    assert!(!valid_args(&["x".repeat(MAX_ARG_BYTES + 1)]));
    for control in ["a\tb", "a\nb", "a\rb", "\u{1b}[31m", "nul\0"] {
        assert!(!valid_args(&[control]), "{control:?}");
    }
}
