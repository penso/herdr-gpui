#![allow(clippy::unwrap_used)]
use super::text;
use crate::control::protocol::{
    app::HostInfo,
    hosts::{
        CheckInfo, CommentInfo, ForwardInfo, LoadInfo, PortInfo, Ports, PrInfo, Reply, UsageInfo,
        UsageWindow,
    },
};

mod remote_files;

fn check(name: &str, outcome: &str) -> CheckInfo {
    CheckInfo {
        name: name.into(),
        outcome: outcome.into(),
    }
}

#[test]
fn a_pull_request_reads_as_a_short_summary() {
    let pr = PrInfo {
        number: 8,
        url: "https://github.com/example/project/pull/8".into(),
        title: "Improve menus".into(),
        state: "Open".into(),
        draft: false,
        head: "feature".into(),
        base: "main".into(),
        additions: 10,
        deletions: 2,
        changed_files: 1,
        review: Some("Review required".into()),
        merge_status: None,
        checks: vec![
            check("test", "passed"),
            check("lint", "failed"),
            check("doc", "passed"),
        ],
    };
    assert_eq!(
        text::pr(&pr),
        "#8 Improve menus\n\
         Open · feature -> main · +10 -2 in 1 file\n\
         Review required\n\
         Checks: 2 passed, 1 failed\n\
         https://github.com/example/project/pull/8"
    );
    assert_eq!(
        text::checks(&pr.checks),
        "passed   test\nfailed   lint\npassed   doc"
    );
    assert_eq!(text::checks(&[]), "No checks reported");
}

#[test]
fn comments_and_hosts_read_one_per_entry() {
    let comments = [CommentInfo {
        author: "octocat".into(),
        kind: "approved".into(),
        created_at: "2026-09-20".into(),
        body: "Ship it".into(),
    }];
    assert_eq!(
        text::comments(&comments),
        "octocat approved (2026-09-20)\n    Ship it"
    );
    assert_eq!(text::comments(&[]), "No comments yet");
    let hosts = [
        HostInfo {
            id: "local".into(),
            label: "Local".into(),
            status: "online".into(),
            selected: true,
        },
        HostInfo {
            id: "ssh:devbox".into(),
            label: "Devbox".into(),
            status: "offline".into(),
            selected: false,
        },
    ];
    assert_eq!(
        text::hosts(&hosts),
        "> Local (local): online\n  Devbox (ssh:devbox): offline"
    );
}

#[test]
fn ports_list_listeners_forwards_and_why_none() {
    let reply = Ports {
        host: "Devbox".into(),
        workspace: "w_1".into(),
        listening: vec![PortInfo {
            port: 3000,
            bind: "*:3000".into(),
            process: Some("node".into()),
            local_url: Some("http://devbox:3000/".into()),
        }],
        forwards: vec![ForwardInfo {
            host: "me@devbox".into(),
            port: 5173,
            state: "ended".into(),
            local_url: None,
            reason: Some("ssh exited".into()),
        }],
        note: None,
    };
    assert_eq!(
        text::ports(&reply),
        "Workspace w_1 on Devbox listens on:\n  \
         *:3000                   node  http://devbox:3000/\n\
         Forwarded:\n  me@devbox:5173 ended (ssh exited)"
    );
    let empty = Ports {
        host: "Local".into(),
        workspace: "w_1".into(),
        listening: Vec::new(),
        forwards: Vec::new(),
        note: Some("Scanning is off".into()),
    };
    assert_eq!(
        text::ports(&empty),
        "Workspace w_1 on Local listens on no port\nScanning is off"
    );
}

#[test]
fn usage_and_load_read_in_plain_units() {
    let reply = Reply::Usage {
        host: "Local".into(),
        usage: vec![UsageInfo {
            provider: "Claude".into(),
            account: Some("me@example.com".into()),
            plan: Some("Max".into()),
            windows: vec![UsageWindow {
                label: "Session".into(),
                used_percent: 12.4,
                resets_in_seconds: Some(2 * 3600 + 53 * 60),
            }],
            error: None,
        }],
        note: None,
    };
    assert_eq!(
        text::usage(&reply),
        "Claude · Max · me@example.com\n  Session       12% used, resets in 2h 53m"
    );
    let load = Reply::System {
        load: Some(Box::new(LoadInfo {
            host: "Local".into(),
            cpu_percent: Some(25.),
            memory_used: Some(8 << 30),
            memory_total: Some(16 << 30),
            cores: Some(8),
            load: None,
            uptime_seconds: Some(3 * 86_400 + 4 * 3600),
            disk_available: None,
            disk_total: None,
        })),
        note: None,
    };
    assert_eq!(
        text::system(&load),
        "Local\n  CPU     25% of 8 cores\n  Memory  8.0 GiB of 16.0 GiB\n  Up      3d 4h"
    );
    let none = Reply::System {
        load: None,
        note: Some("Not sampled".into()),
    };
    assert_eq!(text::system(&none), "Not sampled");
}

#[test]
fn durations_and_sizes_keep_two_units_at_most() {
    assert_eq!(text::duration(59), "0m");
    assert_eq!(text::duration(5 * 60), "5m");
    assert_eq!(text::duration(3600 + 60), "1h 1m");
    assert_eq!(text::duration(2 * 86_400 + 3600 + 60), "2d 1h");
    assert_eq!(text::bytes(512), "512 B");
    assert_eq!(text::bytes(1536), "1.5 KiB");
    assert_eq!(text::bytes(3 << 30), "3.0 GiB");
}

#[cfg(feature = "cloud")]
#[test]
fn cloud_machines_read_one_per_line() {
    use super::cloud::{Machine, text};
    let machines = [Machine {
        provider: "Daytona".into(),
        id: "abc".into(),
        label: "Sandbox".into(),
        account: "team".into(),
        session: "default".into(),
        enabled: false,
    }];
    assert_eq!(
        text(&machines),
        "Daytona Sandbox (abc, team) session default, turned off"
    );
    assert!(text(&[]).contains("cloud add"));
}

#[test]
fn cloud_help_describes_running_setup_again() {
    let help = super::super::help::usage(Some("cloud"));
    assert!(help.contains("herdr-gpui cloud setup ID"), "{help}");
}
