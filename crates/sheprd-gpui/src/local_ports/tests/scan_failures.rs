//! The script's exit status: a tool that failed is not an empty computer.
//! Shell functions stand in for the tools, so these run wherever `/bin/sh`
//! does, whichever tool the computer has.

use super::*;
use crate::local_ports::scan::{SCRIPT, parse};
use sheprd_client::{Error as ClientError, ScriptHost, ScriptLimits, run_script};
use std::sync::atomic::AtomicBool;

/// Only `tool` is installed, and it runs `body`.
fn run(tool: &str, body: &str) -> Result<String, ClientError> {
    let script = format!("command() {{ [ \"$2\" = {tool} ]; }}\n{tool}() {{ {body}; }}\n{SCRIPT}");
    let mut output = Vec::new();
    run_script(
        ScriptHost::Local,
        &script,
        std::io::empty(),
        &mut output,
        ScriptLimits {
            output: 4096,
            idle: Duration::from_secs(15),
        },
        &AtomicBool::new(false),
    )?;
    Ok(String::from_utf8_lossy(&output).into_owned())
}

fn failure(result: Result<String, ClientError>) -> (Option<i32>, String) {
    match result {
        Err(ClientError::ScriptExit { status, stderr }) => (status.code(), stderr),
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_ss_that_fails_is_a_failed_scan_with_its_reason() {
    let (code, stderr) = failure(run("ss", "echo 'Cannot open netlink socket' >&2; return 3"));
    assert_eq!(code, Some(3));
    assert!(stderr.contains("netlink"), "{stderr}");
}

#[test]
fn an_ss_that_lists_nothing_is_an_empty_computer() {
    let text = run("ss", "echo 'State Recv-Q Send-Q Local Peer'").unwrap();
    assert_eq!(parse(&text), Some(Vec::new()));
}

#[test]
fn an_lsof_that_finds_nothing_is_an_empty_computer() {
    let text = run("lsof", "return 1").unwrap();
    assert_eq!(parse(&text), Some(Vec::new()));
}

#[test]
fn an_lsof_that_complains_is_a_failed_scan_with_its_reason() {
    let (code, stderr) = failure(run("lsof", "echo 'lsof: no pwd entry' >&2; return 1"));
    assert_eq!(code, Some(1));
    assert!(stderr.contains("no pwd entry"), "{stderr}");
}

#[test]
fn an_lsof_killed_midway_is_a_failed_scan() {
    let (code, _) = failure(run("lsof", "return 2"));
    assert_eq!(code, Some(2));
}

#[test]
fn an_lsof_listing_reaches_the_parser_past_its_warnings() {
    let text = run(
        "lsof",
        "printf 'p42\\ncnode\\nn127.0.0.1:3000\\n'; echo 'warning' >&2",
    )
    .unwrap();
    assert_eq!(parse(&text).unwrap().len(), 1);
}

#[test]
fn a_computer_with_neither_tool_says_so() {
    let text = run("neither", "return 0").unwrap();
    assert_eq!(parse(&text), None);
}
