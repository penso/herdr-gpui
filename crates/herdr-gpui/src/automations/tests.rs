#![allow(clippy::unwrap_used)]

use super::{Action, Automations, Error, Loaded, Look, Machine, Opened, Read, Wanted};
use crate::{HerdrWindow, sidebar::ActivityPanel, teleport::Host};
use herdr_automation::report::RunState;
use herdr_automation::report::{
    Daemon, HostCaps, Reason, RunSummary, ScriptCaps, ScriptState, ScriptStatus, Status, Tokens,
    TriggerStatus,
};
use std::{sync::Arc, time::SystemTime};

mod actions;
mod autostart;
mod en_xa;
mod notify;
mod parse;
mod review;
mod rows;
mod section;
mod stale;
mod timeline;
mod turned_off;

/// Monday 2026-10-19 12:00:00 UTC.
const NOW: i64 = 1_792_411_200;
const HOUR: i64 = 3600;

pub(crate) fn run(id: i64, script: &str, state: RunState, started_at: i64) -> RunSummary {
    RunSummary {
        id,
        script: format!("/repo/.herdr/automations/{script}.luau"),
        name: script.to_owned(),
        trigger: 0,
        trigger_name: None,
        reason: Reason::Schedule,
        scheduled: Some(started_at),
        started_at,
        finished_at: (state != RunState::Running).then_some(started_at + 41 * 60),
        state,
        detail: (state == RunState::Failed).then(|| "pull: checkout is dirty".to_owned()),
        steps: 3,
        current_step: (state == RunState::Running).then_some(2),
        agents: 1,
        tokens: Some(1_200_000),
    }
}

pub(crate) fn script(name: &str, state: ScriptState) -> ScriptStatus {
    ScriptStatus {
        path: format!("/repo/.herdr/automations/{name}.luau"),
        name: name.to_owned(),
        repo: Some("/repo".to_owned()),
        hash: format!("{name}-hash-0123456789abcdef"),
        state,
        reason: None,
        caps: Some(ScriptCaps {
            agents_per_run: 2,
            tokens_per_day: 2_000_000,
            wall_time_seconds: 3 * 3600,
        }),
        triggers: vec![TriggerStatus {
            index: 0,
            name: None,
            schedule: "daily at 03:00".to_owned(),
            zone: "UTC".to_owned(),
            next: (state == ScriptState::Active).then_some(NOW + 15 * HOUR),
        }],
        plan: Some(format!("{name}\n  daily at 03:00\n    1. pull main\n")),
        last_run: None,
        running: None,
        tokens_today: Some(340_000),
    }
}

/// A host with one script in each interesting state, and a few runs.
pub(crate) fn status() -> Status {
    let mut running = script("nightly", ScriptState::Active);
    running.running = Some(128);
    running.last_run = Some(run(127, "nightly", RunState::Completed, NOW - 33 * HOUR));
    let mut failing = script("triage", ScriptState::Active);
    failing.last_run = Some(run(126, "triage", RunState::Failed, NOW - 3 * HOUR));
    let mut waiting = script("deploy", ScriptState::AwaitingApproval);
    waiting.reason = Some("Changed by git pull".to_owned());
    let paused = script("cleanup", ScriptState::Paused);
    Status {
        protocol: herdr_automation::report::PROTOCOL,
        version: "0.3.1".to_owned(),
        daemon: Daemon {
            running: true,
            pid: Some(42),
            version: Some("0.3.1".to_owned()),
            started_at: Some(NOW - 2 * HOUR),
            beat_at: Some(NOW),
            paused_all: false,
            service: true,
        },
        caps: HostCaps::default(),
        today: Tokens {
            input: 1_000_000,
            output: 400_000,
            cache_read: 0,
            cache_write: 0,
        },
        scripts: vec![running, failing, waiting, paused],
        runs: vec![
            run(128, "nightly", RunState::Running, NOW - 12 * 60),
            run(126, "triage", RunState::Failed, NOW - 3 * HOUR),
            run(127, "nightly", RunState::Completed, NOW - 33 * HOUR),
            run(125, "nightly", RunState::Completed, NOW - 4 * 24 * HOUR),
            run(100, "nightly", RunState::Completed, NOW - 20 * 24 * HOUR),
        ],
    }
}

pub(crate) fn wanted(machine: Machine, label: &str) -> Wanted {
    Wanted {
        machine,
        label: label.to_owned(),
        endpoint: None,
        host: Host::new(&herdr_client::ConnectTarget::Local).unwrap(),
    }
}
