//! Beads syncs on an SSH host in one connection: `bd`'s options, the list,
//! and the blocked beads in one script, run here under a local shell with a
//! fake `bd`.
use super::*;
use crate::beads::{split, sync_script};
use crate::test_executable;
use herdr_remote::teleport::Host;
use std::{process::Command, sync::atomic::AtomicBool, time::Duration};

const SEPARATOR: &str = "0123456789ab";

/// Runs the sync script in a folder whose `bd` is `fake`, and returns its
/// output and the arguments each `bd` call had.
fn sync_with(fake: &str) -> (herdr_client::Result<Vec<u8>>, String) {
    sync_within(fake, Duration::from_secs(10))
}

/// [`sync_with`], ended once it prints nothing for `idle`.
fn sync_within(fake: &str, idle: Duration) -> (herdr_client::Result<Vec<u8>>, String) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project with 'quotes'");
    std::fs::create_dir_all(root.join(".beads")).unwrap();
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let calls = dir.path().join("calls");
    test_executable::write(bin.join("bd"), fake, 0o755).unwrap();
    let mut command = Command::new("/bin/sh");
    command
        .args(["-c", &sync_script(&root.to_string_lossy(), SEPARATOR)])
        .env_clear()
        .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
        .env("CALLS", &calls);
    let output = Host::capture_command(command, idle, &AtomicBool::new(false));
    (output, std::fs::read_to_string(calls).unwrap_or_default())
}

const FAKE: &str = r#"#!/bin/sh
printf '%s|%s\n' "$*" "$BEADS_DIR" >> "$CALLS"
case "$*" in
--help) printf 'Flags:\n      --readonly   Read only\n      --sandboxed  Not the flag\n' ;;
*list*) printf '[{"id":"hg-1","title":"One","status":"open"}]\n' ;;
*blocked*) printf '{"issues":[]}\n' ;;
esac
"#;

#[test]
fn one_script_reads_the_options_the_list_and_the_blockers() {
    let (output, calls) = sync_with(FAKE);
    let (list, blocked) = split(&output.unwrap(), SEPARATOR).unwrap();
    assert_eq!(
        list.trim_ascii(),
        br#"[{"id":"hg-1","title":"One","status":"open"}]"#
    );
    assert_eq!(blocked.trim_ascii(), br#"{"issues":[]}"#);
    let calls: Vec<_> = calls
        .lines()
        .map(|line| line.split('|').next().unwrap())
        .collect();
    // Only the flags `bd`'s help names, as whole words.
    assert_eq!(
        calls,
        [
            "--help",
            "--readonly list --json --all --limit 0",
            "--readonly blocked --json",
        ]
    );
    let items = crate::beads::parse(&beads(), &list, &blocked).unwrap();
    assert_eq!(items.len(), 1);
}

#[test]
fn a_failing_step_ends_the_script_with_its_status() {
    let fake = FAKE.replace(
        "*blocked*) printf '{\"issues\":[]}\\n' ;;",
        "*blocked*) echo no >&2; exit 3 ;;",
    );
    assert_ne!(fake, FAKE);
    let (output, _) = sync_with(&fake);
    let Err(herdr_client::Error::ScriptExit { status, stderr }) = output else {
        panic!("expected a failure, got {output:?}");
    };
    assert_eq!(status.code(), Some(3));
    assert!(stderr.contains("no"));
}

#[test]
fn output_without_the_separator_is_unexpected() {
    assert!(matches!(
        split(b"flags\n[]\n{}\n", SEPARATOR),
        Err(Error::Output { .. })
    ));
}

#[test]
fn each_step_has_the_idle_limit_to_itself() {
    // Quiet for most of the limit each, and for longer than it together:
    // each step's output ends the quiet before the next one starts.
    let fake = FAKE
        .replace("*list*) printf", "*list*) sleep 2.5; printf")
        .replace("*blocked*) printf", "*blocked*) sleep 2.5; printf");
    assert_ne!(fake, FAKE);
    let (output, _) = sync_within(&fake, Duration::from_secs(4));
    let (list, blocked) = split(&output.unwrap(), SEPARATOR).unwrap();
    let items = crate::beads::parse(&beads(), &list, &blocked).unwrap();
    assert_eq!(items.len(), 1);
}

#[test]
fn output_without_the_flags_line_is_unexpected() {
    let output = format!("[]\n{SEPARATOR}\n{{}}\n");
    assert!(matches!(
        split(output.as_bytes(), SEPARATOR),
        Err(Error::Output { .. })
    ));
}

#[test]
fn the_script_has_room_for_both_responses_at_their_own_limits() {
    // Each was its own script's output once, with the separator and the
    // flags line around them now.
    let lines = format!("flags --readonly --sandbox\n\n{SEPARATOR}\n").len() as u64;
    assert!(crate::beads::SYNC_OUTPUT >= 2 * Host::QUERY_OUTPUT + lines);
}
