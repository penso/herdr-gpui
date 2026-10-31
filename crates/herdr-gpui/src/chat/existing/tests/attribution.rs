use super::*;

const SESSION: &str = "12345678-1234-4123-8123-123456789abc";
const OTHER: &str = "87654321-4321-4321-8321-cba987654321";

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn explicit_codex_resume_ids_must_agree_as_uuids() {
    for invocation in [
        vec!["resume", SESSION],
        vec!["resume", SESSION, "a prompt"],
        vec!["--no-alt-screen", "resume", SESSION],
        vec!["-c", "model=example", "resume", "--all", SESSION],
        vec!["--profile=example", "resume", SESSION, "--model", "example"],
        vec!["resume", "--", SESSION, "--last"],
    ] {
        assert!(
            codex::session_matches(&args(&invocation), SESSION),
            "{invocation:?}"
        );
        assert!(
            !codex::session_matches(&args(&invocation), OTHER),
            "{invocation:?}"
        );
    }
    assert!(codex::session_matches(
        &args(&["resume", SESSION]),
        &SESSION.to_uppercase()
    ));
}

#[test]
fn ambiguous_resume_selectors_and_options_keep_the_terminal() {
    for invocation in [
        vec!["resume"],
        vec!["resume", "--last"],
        vec!["resume", "a named session"],
        vec!["resume", SESSION, "--last"],
        vec!["resume", "--last", SESSION],
        vec!["resume", "--unknown", SESSION],
        vec!["--unknown", "resume", SESSION],
        vec!["resume", "--image", SESSION],
        vec!["resume", "--profile"],
        vec!["resume", SESSION, "--profile", "--last"],
        vec!["resume", SESSION, "--all=false"],
        vec!["resume", SESSION, "first prompt", "second prompt"],
    ] {
        assert!(
            !codex::session_matches(&args(&invocation), SESSION),
            "{invocation:?}"
        );
    }
}

#[test]
fn new_and_forked_codex_sessions_do_not_claim_their_arguments_are_current_ids() {
    for invocation in [
        vec![],
        vec!["--no-alt-screen"],
        vec!["--image", "image.png", "a new prompt"],
        vec!["a new prompt", "resume", OTHER],
        vec!["--", "resume", OTHER],
        vec!["--profile", "resume", OTHER],
        vec!["fork", OTHER],
        vec!["fork", "--last"],
    ] {
        assert!(
            codex::session_matches(&args(&invocation), SESSION),
            "{invocation:?}"
        );
    }
}

#[test]
fn an_in_process_switch_cannot_be_guessed_from_launch_arguments() {
    assert!(!codex::session_matches(&args(&["resume", SESSION]), OTHER));
}

#[test]
fn selected_native_child_supplies_the_resume_identity() {
    for (wrapper_session, native_session, expected) in
        [(OTHER, SESSION, true), (SESSION, OTHER, false)]
    {
        let info = process_info(&[
            (10, &["node", "/bin/codex", "resume", wrapper_session]),
            (20, &["/native/codex", "resume", native_session]),
        ]);
        let pid = agent_pid(
            Agent::Codex,
            &info,
            &process_identities(&[(10, 1), (20, 10)]),
        )
        .unwrap();
        let process = info
            .foreground_processes
            .iter()
            .find(|process| process.pid == pid)
            .unwrap();
        assert_eq!(pid, 20);
        assert_eq!(
            codex::session_matches(agent_args(Agent::Codex, &process.argv).unwrap(), SESSION),
            expected
        );
    }
}

#[cfg(target_os = "linux")]
mod host_checks {
    use super::*;

    fn inspect_args(argv: &[&str]) -> Vec<Binding> {
        let (temp, host) = host::fixture(false, false);
        let path = temp.path().join("process.json");
        let mut process: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        process["result"]["process_info"]["foreground_processes"][0]["argv"] =
            serde_json::json!(argv);
        std::fs::write(path, process.to_string()).unwrap();
        inspect_host(&host, &scope(), &["w1:p1".into()], &AtomicBool::new(false)).unwrap()
    }

    #[test]
    fn wrong_daemon_session_cannot_bind_a_verified_codex_process() {
        assert!(inspect_args(&["codex", "resume", OTHER]).is_empty());
        let bindings = inspect_args(&["codex", "resume", SESSION]);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].session_id, SESSION);
        assert_eq!(bindings[0].pid, std::process::id());
    }

    #[test]
    fn recognized_wrappers_check_their_resume_uuid() {
        for launcher in ["node", "nodejs", "bun"] {
            assert!(
                inspect_args(&[launcher, "/opt/@openai/codex/bin/codex.js", "resume", OTHER])
                    .is_empty()
            );
            assert_eq!(
                inspect_args(&[
                    launcher,
                    "/opt/@openai/codex/bin/codex.js",
                    "resume",
                    SESSION
                ])
                .len(),
                1
            );
        }
    }

    #[test]
    fn picker_falls_back_but_new_sessions_keep_existing_verification() {
        assert!(inspect_args(&["codex", "resume", "--last"]).is_empty());
        assert_eq!(inspect_args(&["codex"]).len(), 1);
    }
}
