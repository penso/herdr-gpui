use super::*;

fn values<'a>(log_tails: &'a [Option<String>], run_ids: &'a [Option<String>]) -> Values<'a> {
    Values {
        date: "2026-10-22",
        time: "03:00",
        log_tails,
        run_ids,
    }
}

#[test]
fn tokens_fill_in_text() {
    let tails = [None, Some("error: test failed".to_owned())];
    let values = values(&tails, &[]);
    assert_eq!(
        values.fill("nightly-{date} at {time}: {log_tail:2}"),
        "nightly-2026-10-22 at 03:00: error: test failed"
    );
}

#[test]
fn unknown_and_missing_tokens_stay() {
    let values = values(&[None], &[]);
    assert_eq!(
        values.fill("{log_tail:1} {log_tail:9} {nope} {log_tail:0} {"),
        "{log_tail:1} {log_tail:9} {nope} {log_tail:0} {"
    );
}

#[test]
fn shell_commands_get_output_through_variables() {
    let tails = [Some("$(rm -rf ~); `id`".to_owned())];
    let (command, env) = values(&tails, &[]).fill_shell("echo {log_tail:1} {log_tail:1} on {date}");
    assert_eq!(
        command,
        "echo \"$HERDR_LOG_TAIL_1\" \"$HERDR_LOG_TAIL_1\" on 2026-10-22"
    );
    assert_eq!(
        env,
        [(
            "HERDR_LOG_TAIL_1".to_owned(),
            "$(rm -rf ~); `id`".to_owned()
        )]
    );
}

#[test]
fn resolving_a_call_fills_every_field() {
    let tails = [Some("boom".to_owned())];
    let runs = [Some("pane-7".to_owned())];
    let values = values(&tails, &runs);
    let call = Call {
        step: Step::Agent {
            harness: "claude".to_owned(),
            prompt: "Failed with {log_tail:1}; see {run_id:1}".to_owned(),
            worktree: Some("fix-{date}".to_owned()),
            base: None,
            wait: false,
        },
        check: true,
        timeout: None,
    };
    let resolved = call.resolve(&values);
    assert_eq!(
        resolved.call.step,
        Step::Agent {
            harness: "claude".to_owned(),
            prompt: "Failed with boom; see pane-7".to_owned(),
            worktree: Some("fix-2026-10-22".to_owned()),
            base: None,
            wait: false,
        }
    );
    assert!(resolved.env.is_empty());
}

#[test]
fn output_references_are_quoted_for_where_they_sit() {
    let tails = [Some("a b *".to_owned())];
    let fill = |command: &str| values(&tails, &[]).fill_shell(command).0;
    assert_eq!(
        fill(r#"printf '%s\n' "{log_tail:1}""#),
        r#"printf '%s\n' "${HERDR_LOG_TAIL_1}""#
    );
    assert_eq!(
        fill("echo 'said: {log_tail:1}!'"),
        r#"echo 'said: '"$HERDR_LOG_TAIL_1"'!'"#
    );
    assert_eq!(
        fill(r#"echo "$(printf %s {log_tail:1})""#),
        r#"echo "$(printf %s "$HERDR_LOG_TAIL_1")""#
    );
    assert_eq!(
        fill("echo `echo {log_tail:1}` \"x\" {log_tail:1}"),
        "echo `echo \"$HERDR_LOG_TAIL_1\"` \"x\" \"$HERDR_LOG_TAIL_1\""
    );
    assert_eq!(fill(r"echo \{log_tail:1}"), r"echo \{log_tail:1}");
    assert_eq!(fill("echo \"it's {date}\""), "echo \"it's 2026-10-22\"");
}

#[test]
fn output_tokens_are_refused_where_their_quoting_cannot_be_told() {
    use crate::tokens::shell_tokens_placeable as placeable;
    assert!(placeable("echo # no token here"));
    assert!(placeable(r#"printf '%s' "{log_tail:1}" | tee 'out'"#));
    for command in [
        "echo {log_tail:1} # \"",
        "cat <<EOF\n{log_tail:1}\nEOF",
        "echo $'a' {log_tail:1}",
        "echo ${X:-\"{log_tail:1}\"}",
        "echo \"open {run_id:1}",
        "echo 'open {log_tail:1}",
    ] {
        assert!(!placeable(command), "{command}");
    }
}
