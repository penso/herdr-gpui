//! Dispatching to a bead claims it first and gives a claim back when the
//! agent never started, against a fake `bd` that logs its calls and answers
//! as `bd` 1.3 does.
use super::*;
use crate::{Error, beads::claim, test_executable};

const FAKE_BD: &str = r#"#!/bin/sh
printf '%s|%s\n' "$*" "$BEADS_DIR" >> "$HOME/bd-calls"
case "$*" in
*show*) cat "$HOME/shown" ;;
*--claim*)
    if [ -f "$HOME/refusal" ]; then cat "$HOME/refusal" >&2; exit 1; fi
    printf '[{"id":"tt-1","status":"in_progress","assignee":"me"}]\n'
    ;;
esac
"#;

/// A site whose bead `tt-1` is `shown` by `bd`, and the bead's dispatch.
fn bead_site(dir: &Path, fail: &str, shown: &str) -> (Site, DispatchRequest) {
    let site = site(dir, fail);
    let home = dir.join("home");
    test_executable::write(home.join(".local/bin/bd"), FAKE_BD, 0o755).unwrap();
    std::fs::write(home.join("shown"), shown).unwrap();
    let key = ItemKey {
        source: SourceKey {
            provider: Provider::Beads,
            host: "local".into(),
            repository: site.main_root.clone(),
        },
        native_id: "tt-1".into(),
    };
    let request = DispatchRequest {
        item_key: key.canonical(),
        ..request()
    };
    (site, request)
}

const UNCLAIMED: &str = r#"[{"id":"tt-1","status":"open"}]"#;

fn bd_calls(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("home/bd-calls"))
        .unwrap()
        .lines()
        .map(|line| line.split('|').next().unwrap().to_owned())
        .collect()
}

fn dispatch(site: &Site, request: DispatchRequest) -> Result<Done> {
    perform(
        site,
        &Action::Dispatch(Box::new(request)),
        &AtomicBool::new(false),
    )
}

#[test]
fn a_bead_is_claimed_as_bd_by_hand_would_before_the_agent_starts_and_stays_claimed() {
    let dir = tempfile::tempdir().unwrap();
    let (site, request) = bead_site(dir.path(), "", UNCLAIMED);
    assert_eq!(dispatch(&site, request).unwrap(), Done::Started);
    assert_eq!(
        bd_calls(dir.path()),
        ["show --json -- tt-1", "update --claim --json -- tt-1"]
    );
    // `bd` runs against the source's own database.
    let log = std::fs::read_to_string(dir.path().join("home/bd-calls")).unwrap();
    let database = Path::new(&site.main_root).join(".beads");
    assert!(
        log.lines()
            .all(|line| line.ends_with(database.to_str().unwrap()))
    );
}

#[test]
fn a_held_bead_starts_nothing_and_says_who_holds_it() {
    let dir = tempfile::tempdir().unwrap();
    let (site, request) = bead_site(
        dir.path(),
        "",
        r#"[{"id":"tt-1","status":"in_progress","assignee":"alice"}]"#,
    );
    std::fs::write(
        dir.path().join("home/refusal"),
        "Error updating tt-1: issue already claimed by alice\n",
    )
    .unwrap();
    let error = dispatch(&site, request).unwrap_err();
    assert!(
        matches!(&error, Error::Claim(claim::Error::Held { id, holder })
            if id == "tt-1" && holder == "alice"),
        "{error:?}"
    );
    assert!(calls(dir.path()).is_empty(), "nothing reached Herdr");
    assert!(!site.database.exists(), "no run was recorded");
}

#[test]
fn any_other_refusal_keeps_bds_own_failure() {
    let dir = tempfile::tempdir().unwrap();
    let (site, request) = bead_site(dir.path(), "", UNCLAIMED);
    std::fs::write(
        dir.path().join("home/refusal"),
        "issue not claimable: status closed\n",
    )
    .unwrap();
    let error = dispatch(&site, request).unwrap_err();
    assert!(
        matches!(
            &error,
            Error::Script {
                operation: "Claiming the bead",
                ..
            }
        ),
        "{error:?}"
    );
    assert!(calls(dir.path()).is_empty());
}

#[test]
fn a_launch_that_fails_gives_its_claim_back_and_stays_a_failed_run() {
    let dir = tempfile::tempdir().unwrap();
    let (site, request) = bead_site(dir.path(), "agent start", UNCLAIMED);
    assert!(dispatch(&site, request).is_err());
    let calls = bd_calls(dir.path());
    assert_eq!(
        calls.last().map(String::as_str),
        Some("unclaim --if-assignee me -- tt-1")
    );
    assert!(!calls.iter().any(|call| call.contains("close")));
    let run = Store::open(&site.database)
        .unwrap()
        .runs()
        .unwrap()
        .remove(0);
    assert_eq!(run.state, RunState::Failed);
}

#[test]
fn a_launch_that_fails_leaves_a_claim_it_did_not_take() {
    let dir = tempfile::tempdir().unwrap();
    let (site, request) = bead_site(
        dir.path(),
        "agent start",
        r#"[{"id":"tt-1","status":"in_progress","assignee":"me"}]"#,
    );
    assert!(dispatch(&site, request).is_err());
    assert!(
        !bd_calls(dir.path())
            .iter()
            .any(|call| call.contains("unclaim"))
    );
}

#[test]
fn only_a_bead_is_claimed() {
    let dir = tempfile::tempdir().unwrap();
    let (site, _) = bead_site(dir.path(), "", UNCLAIMED);
    dispatch(&site, request()).unwrap();
    assert!(!dir.path().join("home/bd-calls").exists());
}

#[test]
fn a_bead_dispatch_syncs_once_it_ends() {
    let dir = tempfile::tempdir().unwrap();
    let (_, bead) = bead_site(dir.path(), "", UNCLAIMED);
    assert!(Action::Dispatch(Box::new(bead)).changes_items());
    assert!(!Action::Dispatch(Box::new(request())).changes_items());
}
