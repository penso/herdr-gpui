//! What a script's row says, and what the bar counts.
use super::*;
use crate::automations::{format, model};
use chrono::FixedOffset;

/// `text` without the isolation marks Fluent puts around each placeable.
fn plain(text: String) -> String {
    text.replace(['\u{2068}', '\u{2069}'], "")
}

fn find<'a>(status: &'a Status, name: &str) -> &'a ScriptStatus {
    status
        .scripts
        .iter()
        .find(|script| script.name == name)
        .unwrap()
}

#[test]
fn each_script_shows_its_state() {
    let status = status();
    assert_eq!(model::look(find(&status, "nightly")), Look::Running);
    assert_eq!(model::look(find(&status, "triage")), Look::Failed);
    assert_eq!(model::look(find(&status, "deploy")), Look::Waiting);
    assert_eq!(model::look(find(&status, "cleanup")), Look::Paused);
    let mut invalid = script("broken", ScriptState::Invalid);
    invalid.reason = Some("line 3: unknown step".into());
    assert_eq!(model::look(&invalid), Look::Failed);
    assert_eq!(
        plain(model::detail(&invalid, &status, NOW)),
        "line 3: unknown step"
    );
}

#[test]
fn each_row_says_what_runs_waits_or_fails() {
    let status = status();
    let now = NOW;
    assert_eq!(
        plain(model::detail(find(&status, "nightly"), &status, now)),
        "Step 2/3 · 12m00s"
    );
    assert_eq!(
        plain(model::detail(find(&status, "triage"), &status, now)),
        "Failed: pull: checkout is dirty"
    );
    assert_eq!(
        plain(model::detail(find(&status, "deploy"), &status, now)),
        "Changed by git pull · awaiting approval"
    );
    assert_eq!(
        plain(model::detail(find(&status, "cleanup"), &status, now)),
        "Paused"
    );
    assert!(model::needs_you(find(&status, "triage")));
    assert!(model::needs_you(find(&status, "deploy")));
    assert!(!model::needs_you(find(&status, "nightly")));
    assert!(!model::needs_you(find(&status, "cleanup")));
}

#[test]
fn the_bar_counts_approvals_and_failures_across_hosts_and_pulses_while_running() {
    let mut automations = Automations::default();
    assert_eq!(automations.needing(), 0);
    assert_eq!(automations.needing_since(), None);
    automations.install_for_test(
        wanted(Machine::Local, "This computer"),
        Read::Ready(Arc::new(status())),
    );
    automations.install_for_test(
        wanted(Machine::Ssh("buildbox".into()), "buildbox"),
        Read::Ready(Arc::new(status())),
    );
    // A host without the program, or not read yet, counts nothing.
    automations.install_for_test(wanted(Machine::Ssh("pi".into()), "pi"), Read::Missing);
    assert_eq!(automations.needing(), 4);
    assert_eq!(automations.running(), 2);
    let failed_end = NOW - 3 * HOUR + 41 * 60;
    assert_eq!(
        automations.needing_since().map(|at| at.timestamp()),
        Some(failed_end)
    );
}

#[test]
fn a_failed_read_keeps_what_was_read_before() {
    let mut automations = Automations::default();
    automations.install_for_test(wanted(Machine::Local, "here"), Read::Unread);
    automations.install(&Machine::Local, Ok(status()), SystemTime::now());
    assert!(
        automations
            .host(&Machine::Local)
            .unwrap()
            .status()
            .is_some()
    );
    automations.install(
        &Machine::Local,
        Err(Error::Refused("busy".into())),
        SystemTime::now(),
    );
    let host = automations.host(&Machine::Local).unwrap();
    assert!(matches!(host.read, Read::Failed { .. }));
    assert_eq!(host.status().map(|status| status.scripts.len()), Some(4));
    // A missing program is not a failure: there is nothing to keep.
    automations.install(&Machine::Local, Err(Error::Missing), SystemTime::now());
    assert!(matches!(
        automations.host(&Machine::Local).unwrap().read,
        Read::Missing
    ));
}

#[test]
fn an_open_script_closes_when_its_host_no_longer_lists_it() {
    let mut automations = Automations::default();
    automations.install_for_test(wanted(Machine::Local, "here"), Read::Unread);
    automations.install(&Machine::Local, Ok(status()), SystemTime::now());
    automations.opened = Some(Opened::Script {
        machine: Machine::Local,
        path: find(&status(), "deploy").path.clone(),
    });
    let mut changed = status();
    changed.scripts.retain(|script| script.name != "deploy");
    automations.install(&Machine::Local, Ok(changed), SystemTime::now());
    assert_eq!(automations.opened, None);
}

#[test]
fn times_durations_and_counts_read_short() {
    let utc = FixedOffset::east_opt(0).unwrap();
    assert_eq!(format::when_in(NOW + 15 * 60, NOW, &utc), "12:15");
    assert_eq!(format::when_in(NOW - 33 * HOUR, NOW, &utc), "Sun 03:00");
    assert_eq!(format::when_in(NOW - 20 * 24 * HOUR, NOW, &utc), "Sep 29");
    assert_eq!(plain(format::until(NOW + 72 * 60, NOW)), "in 1 h 12 min");
    assert_eq!(plain(format::until(NOW + 5 * 60, NOW)), "in 5 min");
    assert_eq!(plain(format::span(3)), "3s");
    assert_eq!(plain(format::span(12 * 60 + 4)), "12m04s");
    assert_eq!(plain(format::span(2 * HOUR + 4 * 60)), "2h 04m");
    assert_eq!(plain(format::span(4 * 24 * HOUR)), "4 d");
    assert_eq!(format::tokens(940), "940");
    assert_eq!(format::tokens(340_000), "340k");
    assert_eq!(format::tokens(1_200_000), "1.2M");
    assert_eq!(format::tokens(10_000_000), "10M");
    assert_eq!(plain(format::bytes(3_300_000_000)), "3.1 GB");
    assert_eq!(plain(format::cpu(700_000)), "11m40s");
}

#[test]
fn a_runs_usage_adds_up_what_its_steps_measured() {
    use crate::automations::page::total_usage;
    use herdr_automation::report::{StepRecord, StepState, Usage};
    let step = |tokens: Option<u64>, cpu: Option<u64>, memory: Option<u64>| StepRecord {
        position: 1,
        kind: "agent".into(),
        summary: String::new(),
        started_at: NOW,
        finished_at: None,
        state: StepState::Succeeded,
        exit: None,
        log_tail: String::new(),
        worktree: None,
        pane: None,
        usage: Usage {
            tokens: tokens.map(|input| Tokens {
                input,
                ..Tokens::default()
            }),
            cpu_ms: cpu,
            peak_memory: memory,
        },
    };
    let total = total_usage(&[
        step(Some(10), None, Some(5)),
        step(Some(5), Some(100), Some(9)),
        step(None, None, None),
    ]);
    assert_eq!(total.tokens.map(Tokens::total), Some(15));
    assert_eq!(total.cpu_ms, Some(100));
    assert_eq!(total.peak_memory, Some(9));
    // Nothing measured stays unknown, never zero.
    assert_eq!(total_usage(&[step(None, None, None)]), Usage::default());
}

#[test]
fn the_token_chart_counts_the_last_week_by_day() {
    let utc = FixedOffset::east_opt(0).unwrap();
    let status = status();
    let runs: Vec<&RunSummary> = status.runs.iter().collect();
    let days = crate::automations::page::daily_tokens(&runs, NOW, &utc);
    assert_eq!(days.len(), 7);
    assert_eq!(days.last().map(|(day, _)| day.as_str()), Some("Mon"));
    // Today: the running run and the failed one; Sunday one; Thursday one;
    // the run 20 days ago is outside the week.
    assert_eq!(days[6].1, 2_400_000);
    assert_eq!(days[5].1, 1_200_000);
    assert_eq!(
        days.iter().map(|(_, tokens)| tokens).sum::<u64>(),
        4_800_000
    );
}
