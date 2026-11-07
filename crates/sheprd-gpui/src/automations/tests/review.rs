//! What a host reports is cleaned once, approving shows all it approves,
//! local commands leave the caller's pane behind, and updates and polling
//! behave.
use super::*;
use crate::automations::{clean, fetch, install};
use gpui::px;
use sheprd_automation::report::{RunDetail, StepRecord, StepState, Usage};
use std::time::{Duration, Instant};

#[test]
fn every_reported_string_is_cleaned_and_bounded_once() {
    let mut status = status();
    let script = &mut status.scripts[0];
    script.name = "night\u{1b}[2Jly\u{202e}".into();
    script.reason = Some("x".repeat(10_000));
    script.plan = Some("line one\u{7}\n  └ two\tthree\n".into());
    script.triggers[0].schedule = "daily\rat 03:00".into();
    status.runs[0].detail = Some("bad\u{0}".into());
    let cleaned = clean::status(status);
    let script = &cleaned.scripts[0];
    assert_eq!(script.name, "night[2Jly");
    assert_eq!(script.reason.as_ref().map(String::len), Some(2000));
    // A plan keeps its lines and tabs, nothing else unsafe.
    assert_eq!(script.plan.as_deref(), Some("line one\n  └ two\tthree\n"));
    assert_eq!(script.triggers[0].schedule, "dailyat 03:00");
    assert_eq!(cleaned.runs[0].detail.as_deref(), Some("bad"));
}

#[test]
fn a_plan_too_long_to_show_whole_is_dropped_not_cut() {
    assert_eq!(clean::plan(Some("a".repeat(clean::PLAN + 1))), None);
    assert_eq!(
        clean::plan(Some("a".repeat(clean::PLAN))).map(|plan| plan.len()),
        Some(clean::PLAN)
    );
}

#[test]
fn a_steps_output_keeps_its_end() {
    let step = StepRecord {
        position: 1,
        kind: "sh\u{1b}".into(),
        summary: "just ci".into(),
        started_at: NOW,
        finished_at: None,
        state: StepState::Running,
        exit: None,
        log_tail: format!("{}\nlast line\u{1b}[0m", "x".repeat(100_000)),
        worktree: None,
        pane: Some("p\u{1}1".into()),
        usage: Usage::default(),
    };
    let detail = clean::detail(RunDetail {
        run: run(1, "nightly", RunState::Running, NOW),
        steps: vec![step],
    });
    let step = &detail.steps[0];
    assert_eq!(step.kind, "sh");
    assert_eq!(step.pane.as_deref(), Some("p1"));
    assert_eq!(step.log_tail.chars().count(), 64 * 1024);
    assert!(step.log_tail.ends_with("\nlast line[0m"));
}

#[test]
fn local_commands_leave_the_callers_pane_behind() {
    let host = Host::new(&sheprd_client::ConnectTarget::Local).unwrap();
    let command = fetch::local_command(&host, &["approve", "/x.luau", "--hash", "h"]);
    let removed = command
        .get_envs()
        .any(|(key, value)| key == "HERDR_PANE_ID" && value.is_none());
    assert!(removed, "HERDR_PANE_ID must be removed");
    let args: Vec<_> = command.get_args().collect();
    assert_eq!(args, ["approve", "/x.luau", "--hash", "h"]);
}

#[test]
fn preparing_an_install_needs_the_shipped_version() {
    let host = Host::new(&sheprd_client::ConnectTarget::Local).unwrap();
    let cancelled = std::sync::atomic::AtomicBool::new(false);
    assert!(matches!(
        install::prepare(&host, None, &cancelled),
        Err(Error::UnknownVersion)
    ));
}

#[test]
fn a_host_without_the_program_is_read_only_when_asked() {
    let mut automations = Automations::default();
    automations.install_for_test(wanted(Machine::Ssh("pi".into()), "pi"), Read::Missing);
    automations.install_for_test(
        wanted(Machine::Local, "here"),
        Read::Ready(Arc::new(status())),
    );
    let later = Instant::now() + Duration::from_secs(3600);
    let due = |automations: &Automations| -> Vec<Machine> {
        automations
            .hosts
            .iter()
            .filter(|host| host.due(later, Duration::from_secs(60)))
            .map(|host| host.machine.clone())
            .collect()
    };
    assert_eq!(due(&automations), [Machine::Local]);
    automations.recheck_missing();
    assert_eq!(due(&automations).len(), 2);
    automations.install(
        &Machine::Ssh("pi".into()),
        Err(Error::Missing),
        SystemTime::now(),
    );
    automations.refresh();
    assert_eq!(due(&automations).len(), 2);
}

#[gpui::test]
fn an_update_compares_the_running_daemon(cx: &mut gpui::TestAppContext) {
    let (view, cx) = section::window(cx);
    view.update(cx, |view, _| {
        view.automations.local_version = Some("0.4.0".into());
        let mut old = status();
        // The program was updated but the old daemon still runs.
        old.version = "0.4.0".into();
        old.daemon.version = Some("0.3.1".into());
        view.automations.install_for_test(
            wanted(Machine::Ssh("box".into()), "box"),
            Read::Ready(Arc::new(old.clone())),
        );
        let host = view.automations.host(&Machine::Ssh("box".into())).unwrap();
        assert_eq!(view.automation_update(host), Some("0.4.0".into()));
        // With no daemon running, the installed program is what counts.
        old.daemon.running = false;
        view.automations.install_for_test(
            wanted(Machine::Ssh("box".into()), "box"),
            Read::Ready(Arc::new(old)),
        );
        let host = view.automations.host(&Machine::Ssh("box".into())).unwrap();
        assert_eq!(view.automation_update(host), None);
    });
}

#[gpui::test]
fn approving_shows_the_whole_hash_and_needs_a_plan(cx: &mut gpui::TestAppContext) {
    let (view, cx) = section::window(cx);
    let mut planless = status();
    planless.scripts[2].plan = None;
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.automations.install_for_test(
                wanted(Machine::Local, "here"),
                Read::Ready(Arc::new(planless)),
            );
            view.review_automation(
                &Machine::Local,
                "/repo/.herdr/automations/deploy.luau",
                window,
                cx,
            );
        });
    });
    section::draw(cx);
    assert!(cx.debug_bounds("automation-approve-no-plan").is_some());
    assert!(cx.debug_bounds("automation-approve-submit").is_none());
    assert!(cx.debug_bounds("automation-approve-cancel").is_some());
}

#[gpui::test]
fn a_long_plan_is_shown_whole(cx: &mut gpui::TestAppContext) {
    let (view, cx) = section::window(cx);
    let mut long = status();
    let plan: String = (0..300)
        .map(|line| format!("{line}. {}\n", "step ".repeat(40)))
        .collect();
    long.scripts[2].plan = Some(plan);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.automations
                .install_for_test(wanted(Machine::Local, "here"), Read::Ready(Arc::new(long)));
            view.review_automation(
                &Machine::Local,
                "/repo/.herdr/automations/deploy.luau",
                window,
                cx,
            );
        });
    });
    section::draw(cx);
    let plan = cx.debug_bounds("automation-approve-plan").unwrap();
    let dialog = cx.debug_bounds("automation-approve").unwrap();
    // Every line is laid out (the block is taller than its scroller), and
    // long lines wrap inside the dialog.
    assert!(plan.size.height > px(300. * 10.));
    assert!(plan.right() <= dialog.right());
    assert!(cx.debug_bounds("automation-approve-submit").is_some());
}
