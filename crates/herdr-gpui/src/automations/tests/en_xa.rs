//! The `en-XA` leak test (`crate::i18n::leak_tests`) for Automations: the
//! host cards, the timeline, a script's page, a run's page with its steps,
//! and the dialogs that approve a script and install `herdr-automate`.
use super::*;
use crate::{
    automations::{
        dialog::{Enable, Stage},
        install::Plan,
    },
    host_build::Platform,
    i18n::leak_tests::{assert_translated, uncut, under_en_xa},
};
use gpui::{Entity, VisualTestContext};
use herdr_automation::report::{RunDetail, StepRecord, StepState, Usage};

/// What the hosts and their scripts report: names, paths, schedules,
/// plans, the daemon's own words, and the versions and hashes approved.
fn data() -> Vec<String> {
    let status = status();
    let mut data: Vec<String> = [
        "pi",
        "SSH",
        "nightly",
        "triage",
        "deploy",
        "cleanup",
        "daily at 03:00",
        "/repo",
        "pull main",
        "just ci",
        "Changed by git pull",
        "pull: checkout is dirty",
        "git pull --ff-only",
        "Already up to date.",
        // Token counts, short as the daemon's reports write them.
        "340k",
        "1.2M",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for script in &status.scripts {
        data.push(script.path.clone());
        data.push(script.hash.clone());
        data.extend(
            script
                .plan
                .iter()
                .flat_map(|plan| plan.lines().map(str::trim).map(str::to_owned)),
        );
    }
    data
}

/// A date as chrono writes it in English (`Oct 19`, `Wed 03:00`), which
/// docs/i18n.md lists as left for ICU4X, as checkpoints' dates are.
fn is_date(line: &str) -> bool {
    const NAMES: [&str; 19] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec", "Mon",
        "Tue", "Wed", "Thu", "Fri", "Sat", "Sun",
    ];
    let mut words = line.split_whitespace();
    words.next().is_some_and(|name| NAMES.contains(&name))
        && words.all(|word| word.chars().all(|c| c.is_ascii_digit() || c == ':'))
}

/// A line that joins facts with " · " is checked fact by fact, each a
/// message or data of its own.
fn facts(shown: Vec<String>) -> Vec<String> {
    uncut(shown)
        .into_iter()
        .flat_map(|line| {
            if herdr_i18n::pseudo::is_marked(&line) {
                vec![line]
            } else {
                line.split(" · ").map(str::to_owned).collect()
            }
        })
        .filter(|line| !is_date(line.trim()))
        .collect()
}

fn step(position: u32, kind: &str, summary: &str, exit: Option<i32>) -> StepRecord {
    StepRecord {
        position,
        kind: kind.into(),
        summary: summary.into(),
        started_at: NOW - 40 * 60,
        finished_at: Some(NOW - 39 * 60),
        state: if exit.is_some() {
            StepState::Failed
        } else {
            StepState::Succeeded
        },
        exit,
        log_tail: "Already up to date.".into(),
        worktree: Some("/repo".into()),
        pane: None,
        usage: Usage {
            tokens: None,
            cpu_ms: Some(1200),
            peak_memory: Some(3_300_000),
        },
    }
}

fn open(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, opened: Option<Opened>) {
    view.update(cx, |view, cx| {
        view.automations.opened = opened;
        cx.notify();
    });
    section::draw(cx);
}

#[test]
fn automations_show_only_messages_and_what_hosts_report() {
    under_en_xa("Automations en-XA leak", |cx, recorder| {
        let (view, cx) = section::window(cx);
        section::draw(cx);
        recorder.take();
        let data = data();

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.automations.install_for_test(
                    wanted(Machine::Local, "pi"),
                    Read::Ready(Arc::new(status())),
                );
                view.automations
                    .install_for_test(wanted(Machine::Ssh("pi".into()), "pi"), Read::Missing);
                view.show_activity_panel(ActivityPanel::Automations, window, cx);
            });
        });
        section::draw(cx);
        assert_translated("host cards", &facts(recorder.take()), &data);

        view.update(cx, |view, cx| {
            view.automations.mode = super::super::Mode::Timeline;
            cx.notify();
        });
        section::draw(cx);
        assert_translated("timeline", &facts(recorder.take()), &data);

        let path = "/repo/.herdr/automations/nightly.luau".to_owned();
        open(
            &view,
            cx,
            Some(Opened::Script {
                machine: Machine::Local,
                path: path.clone(),
            }),
        );
        assert!(cx.debug_bounds("automations-script-page").is_some());
        assert_translated("a script's page", &facts(recorder.take()), &data);

        view.update(cx, |view, _| {
            let mut run = run(127, "nightly", RunState::Failed, NOW - 40 * 60);
            run.trigger_name = Some("nightly".into());
            let detail = RunDetail {
                run,
                steps: vec![
                    step(1, "pull", "git pull --ff-only", None),
                    step(2, "sh", "just ci", Some(101)),
                ],
            };
            view.automations
                .runs
                .insert((Machine::Local, 127), Loaded::Read(Arc::new(detail)));
        });
        open(
            &view,
            cx,
            Some(Opened::Run {
                machine: Machine::Local,
                path,
                run: 127,
            }),
        );
        assert!(cx.debug_bounds("automations-usage").is_some());
        let mut steps = data.clone();
        steps.extend([
            "1. pull".to_owned(),
            "2. sh".to_owned(),
            "nightly #127".to_owned(),
        ]);
        assert_translated("a run's page", &facts(recorder.take()), &steps);

        open(&view, cx, None);
        recorder.take();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.review_automation(
                    &Machine::Local,
                    "/repo/.herdr/automations/deploy.luau",
                    window,
                    cx,
                );
            });
        });
        section::draw(cx);
        assert!(cx.debug_bounds("automation-approve-submit").is_some());
        assert_translated("approving a script", &facts(recorder.take()), &data);

        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.automations.dialog = None;
                view.dismiss_menu(window, cx);
                let plan = Plan {
                    platform: Platform::parse("Linux x86_64").unwrap(),
                    source: "/app/herdr-automate".into(),
                    version: "0.3.1".into(),
                    size: 3_300_000,
                    sha256: "0123456789abcdef".into(),
                };
                let dialog = super::super::Dialog::Enable(Enable {
                    machine: Machine::Ssh("pi".into()),
                    host: "pi".into(),
                    update: false,
                    stage: Stage::Ready(plan),
                });
                view.open_automation_dialog(dialog, window, cx);
            });
        });
        section::draw(cx);
        assert!(cx.debug_bounds("automation-enable-submit").is_some());
        assert_translated("installing herdr-automate", &facts(recorder.take()), &data);
    });
}
