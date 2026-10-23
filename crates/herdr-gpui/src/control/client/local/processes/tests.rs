#![allow(clippy::unwrap_used)]
use super::*;
use crate::processes::{Entry, Identity, Row};

fn row(pid: u32, parent: Option<u32>, depth: usize, cpu: Option<f32>) -> Row {
    Row {
        entry: Entry {
            identity: Identity { pid, started: 1 },
            parent,
            name: if depth == 0 { "zsh" } else { "cargo" }.into(),
            command: if depth == 0 {
                "zsh"
            } else {
                "cargo test --locked"
            }
            .into(),
            cpu,
            memory: 3 << 20,
        },
        depth,
        foreground: depth > 0,
    }
}

fn listing(cpu: Option<f32>) -> Listing {
    Listing {
        rows: vec![row(100, Some(1), 0, cpu), row(200, Some(100), 1, cpu)],
        hidden: 0,
        cpu: cpu.unwrap_or(0.) * 2.,
        memory: 6 << 20,
    }
}

#[test]
fn the_tree_reads_indented_with_usage_and_totals() {
    let tree = Tree::new("w_1:p1", &listing(Some(1.5)));
    assert_eq!(tree.processes[1].parent, Some(100));
    assert_eq!(
        text(&tree),
        "     PID    CPU     MEMORY  COMMAND\n\
         \x20    100   1.5%    3.0 MiB  zsh\n\
         \x20    200   1.5%    3.0 MiB    *cargo  cargo test --locked\n\
         2 processes under pane w_1:p1, 3.0% CPU, 6.0 MiB"
    );
    let json = serde_json::to_value(&tree).unwrap();
    assert_eq!(json["processes"][1]["pid"], 200);
    assert_eq!(json["processes"][1]["foreground"], true);
}

#[test]
fn unmeasured_cpu_shows_a_dash() {
    let tree = Tree::new("p", &listing(None));
    assert!(text(&tree).lines().nth(1).unwrap().contains("     -"));
}

#[test]
fn sampling_waits_for_measured_cpu_or_returns_the_latest() {
    let (watch, sender, _commands) = Watch::fake();
    sender.send(Update::Listing(Ok(listing(None)))).unwrap();
    sender.send(Update::Listing(Ok(listing(Some(2.))))).unwrap();
    let measured = sample(&watch, Duration::from_secs(5)).unwrap();
    assert_eq!(measured.rows[0].entry.cpu, Some(2.));

    let (watch, sender, _commands) = Watch::fake();
    sender.send(Update::Listing(Ok(listing(None)))).unwrap();
    let latest = sample(&watch, Duration::ZERO).unwrap();
    assert_eq!(latest.rows[0].entry.cpu, None);

    let (watch, sender, _commands) = Watch::fake();
    sender
        .send(Update::Listing(Err(crate::processes::Error::NoRoot.into())))
        .unwrap();
    assert!(matches!(
        sample(&watch, Duration::from_secs(5)),
        Err(crate::Error::Processes(crate::processes::Error::NoRoot))
    ));

    let (watch, _sender, _commands) = Watch::fake();
    assert!(matches!(
        sample(&watch, Duration::ZERO),
        Err(crate::Error::Control(crate::control::Error::Rejected {
            code: ErrorCode::Timeout,
            ..
        }))
    ));
}
