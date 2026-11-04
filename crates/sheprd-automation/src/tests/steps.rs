use super::*;

fn calls(handler: &str) -> Vec<Call> {
    record(&format!("herdr.manual(function(fire)\n{handler}\nend)"), 0).0
}

fn run_refusal(handler: &str) -> Refusal {
    let script = load(&format!("herdr.manual(function(fire)\n{handler}\nend)")).unwrap();
    refusal(script.run::<Infallible>(0, |_| Verdict::Succeeded))
}

fn invalid(function: &'static str, problem: Problem) -> Refusal {
    Refusal::Invalid { function, problem }
}

#[test]
fn each_step_reads_its_arguments() {
    let steps = calls(
        "pull(\"main\")\n\
         run(\"setup\", { check = false, timeout = \"30m\" })\n\
         sh(\"cargo build\")\n\
         worktree { name = \"spike\", base = \"main\" }\n\
         agent { harness = \"claude\", prompt = \"line one\\nline two\", worktree = \"fix\", wait = true }\n\
         notify(\"done\")",
    );
    let expected = [
        Step::Pull {
            branch: "main".to_owned(),
        },
        Step::Run {
            name: "setup".to_owned(),
        },
        Step::Sh {
            command: "cargo build".to_owned(),
        },
        Step::Worktree {
            name: "spike".to_owned(),
            base: Some("main".to_owned()),
        },
        Step::Agent {
            harness: "claude".to_owned(),
            prompt: "line one\nline two".to_owned(),
            worktree: Some("fix".to_owned()),
            base: None,
            wait: true,
        },
        Step::Notify {
            text: "done".to_owned(),
        },
    ];
    assert_eq!(
        steps
            .iter()
            .map(|call| call.step.clone())
            .collect::<Vec<_>>(),
        expected
    );
    let run = steps.get(1).unwrap();
    assert!(!run.check);
    assert_eq!(run.timeout, Some(std::time::Duration::from_secs(1_800)));
    assert!(steps.first().unwrap().check);
}

#[test]
fn arguments_are_checked() {
    assert_eq!(
        run_refusal("agent { harness = \"claude\", prompt = \"x\", model = \"big\" }"),
        invalid("agent", Problem::Unknown("model".to_owned()))
    );
    assert_eq!(
        run_refusal("agent { harness = \"claude\" }"),
        invalid("agent", Problem::Missing("prompt"))
    );
    assert_eq!(
        run_refusal("pull(\"ma\\nin\")"),
        invalid("pull", Problem::Control("branch"))
    );
    assert_eq!(
        run_refusal("pull(\"  \")"),
        invalid("pull", Problem::Empty("branch"))
    );
    assert_eq!(
        run_refusal("sh(string.rep(\"x\", 5000))"),
        invalid(
            "sh",
            Problem::TooLong {
                field: "command",
                limit: 4_096,
            }
        )
    );
    assert!(matches!(
        run_refusal("run(\"run\", \"fast\")"),
        Refusal::Invalid {
            problem: Problem::Type {
                field: "options",
                ..
            },
            ..
        }
    ));
    assert!(matches!(
        run_refusal("pull(\"main\", { timeout = \"forever\" })"),
        Refusal::Invalid {
            problem: Problem::Duration(_),
            ..
        }
    ));
}

#[test]
fn invisible_characters_are_refused() {
    // A right-to-left override could make the plan read differently from
    // what runs.
    assert_eq!(
        run_refusal("sh(\"echo safe \\u{202e}fr- mr\")"),
        invalid("sh", Problem::Format("command"))
    );
    assert_eq!(
        run_refusal("agent { harness = \"claude\", prompt = \"hi\\u{200b}\" }"),
        invalid("agent", Problem::Format("prompt"))
    );
    // A variation selector is no format character, but still invisible.
    assert_eq!(
        run_refusal("notify(\"ok\\u{fe0f}\")"),
        invalid("notify", Problem::Format("text"))
    );
    assert_eq!(
        run_refusal("notify(\"tag\\u{e0041}\")"),
        invalid("notify", Problem::Format("text"))
    );
}

#[test]
fn names_must_be_safe_git_references() {
    for bad in [
        "pull(\"-x\")",
        "pull(\"a..b\")",
        "pull(\"main.lock\")",
        "pull(\"feat branch\")",
        "worktree { name = \"fix-{log_tail:1}\" }",
        "agent { harness = \"claude\", prompt = \"x\", worktree = \"w/{run_id:1}\" }",
    ] {
        assert!(
            matches!(
                run_refusal(bad),
                Refusal::Invalid {
                    problem: Problem::Shape { .. },
                    ..
                }
            ),
            "{bad}"
        );
    }
    let ok = calls("worktree { name = \"nightly/{date}\", base = \"origin/main\" }");
    assert_eq!(ok.len(), 1);
}

#[test]
fn harnesses_are_lowercase_names() {
    assert!(matches!(
        run_refusal("agent { harness = \"Claude Code\", prompt = \"x\" }"),
        Refusal::Invalid {
            problem: Problem::Shape {
                field: "harness",
                ..
            },
            ..
        }
    ));
}

#[test]
fn run_takes_only_worktree_scripts() {
    assert!(matches!(
        run_refusal("run(\"validation\")"),
        Refusal::Invalid {
            problem: Problem::Range { field: "name", .. },
            ..
        }
    ));
}

#[test]
fn a_checked_failure_stops_the_run() {
    let script = load("herdr.manual(function() pull(\"main\") notify(\"never\") end)").unwrap();
    let mut seen = 0;
    let end = script
        .run::<Infallible>(0, |_| {
            seen += 1;
            Verdict::Failed
        })
        .unwrap();
    assert_eq!(end, End::Stopped { step: 0 });
    assert_eq!(seen, 1);
}

#[test]
fn an_unchecked_failure_reaches_the_script_as_tokens() {
    let script = load(
        "herdr.manual(function()\n\
           local v = run(\"run\", { check = false })\n\
           assert(v.exit == nil and v.duration == nil)\n\
           if not v.ok then notify(\"failed: \" .. v.log_tail) end\n\
         end)",
    )
    .unwrap();
    let mut texts = Vec::new();
    let end = script
        .run::<Infallible>(0, |call| match &call.step {
            Step::Notify { text } => {
                texts.push(text.clone());
                Verdict::Succeeded
            }
            _ => Verdict::Failed,
        })
        .unwrap();
    assert_eq!(end, End::Completed);
    assert_eq!(texts, ["failed: {log_tail:1}"]);
}

#[test]
fn the_runner_can_halt_a_run() {
    let script = load("herdr.manual(function() notify(\"a\") notify(\"b\") end)").unwrap();
    let end = script
        .run(0, |call| match &call.step {
            Step::Notify { text } if text == "b" => Verdict::Halt("over the cap"),
            _ => Verdict::Succeeded,
        })
        .unwrap();
    assert_eq!(
        end,
        End::Halted {
            step: 1,
            halt: "over the cap",
        }
    );
}

#[test]
fn calls_read_back_in_plain_words() {
    let steps = calls(
        "sh(\"rm -rf target && cargo build --release --workspace --all-features --locked\")\n\
         agent { harness = \"codex\", prompt = \"abc\", worktree = \"w\", base = \"main\" }\n\
         run(\"run\", { check = false })",
    );
    let lines = steps.iter().map(ToString::to_string).collect::<Vec<_>>();
    assert_eq!(
        lines,
        [
            "sh `rm -rf target && cargo build --release --workspace --all-fea…` (arbitrary command)",
            "agent codex in worktree w off main, prompt 3 chars",
            "run worktree script run · the script handles failure",
        ]
    );
}

#[test]
fn a_command_whose_output_quoting_cannot_be_told_is_refused() {
    let refused = run_refusal(
        "local v = sh(\"just ci\", { check = false })\nsh(\"echo \" .. v.log_tail .. \" # done\")",
    );
    assert!(
        matches!(
            refused,
            Refusal::Invalid {
                function: "sh",
                problem: Problem::Shape {
                    field: "command",
                    ..
                },
            }
        ),
        "{refused:?}"
    );
    assert_eq!(calls("sh(\"echo hi # a comment\")").len(), 1);
}
