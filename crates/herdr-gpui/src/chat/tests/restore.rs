//! Continuing chats in panes Herdr restored: which records qualify, the
//! idle-shell check, scheduling, watching live chats end, the claim that
//! keeps a chat from starting twice, and the line typed into the shell.

use super::*;
use crate::chat::restore::{
    Outcome,
    plan::{
        self, CONFIRM_WITHIN, ENDED_AFTER, MAX_PROBES, MAX_STAMPS, PASS_INTERVAL, PASSES, Pane,
        Probes, Scope, Settled, Typed, Watched,
    },
    spawn,
};
use crate::teleport::ProcessInfo;
use herdr_chat_protocol::{Agent, EndpointId, restore::Record};
use herdr_client::protocol::{ClientShellPane, ClientShellSnapshot};
use serde_json::json;
use std::collections::HashSet;
use std::path::Path;

const ONE: &str = "0123456789abcdef0123456789abcdef";
const TWO: &str = "fedcba9876543210fedcba9876543210";
const SOCKET: &str = "/home/me/.config/herdr/herdr.sock";
/// `path` made absolute on this platform: Windows needs a drive.
fn abs(path: &str) -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(format!("C:{path}"))
    } else {
        PathBuf::from(path)
    }
}

const PROGRAM: &str = "/Applications/Herdr.app/Contents/MacOS/herdr-acp";

fn record(endpoint: &str, pane: &str) -> Record {
    Record {
        adapter: false,
        agent: Agent::Claude,
        cwd: abs("/work"),
        endpoint: endpoint.parse().unwrap(),
        pane: pane.to_owned(),
        session: "0c6f0c39-3c1e-4b7e-9a51-6d1b8e2d7f10".to_owned(),
        socket: abs(SOCKET),
    }
}

fn pane(id: &str, occupied: bool) -> Pane {
    Pane {
        cwd: Some("/work".to_owned()),
        id: id.to_owned(),
        occupied,
    }
}

fn ours(socket: &Path) -> bool {
    socket == abs(SOCKET)
}

fn scope(boot: &str, connection: u64) -> Scope {
    Scope {
        boot: boot.to_owned(),
        connection,
        selection: 0,
    }
}

fn endpoint(id: &str) -> EndpointId {
    id.parse().unwrap()
}

fn planned(records: Vec<(Record, Option<String>)>, panes: &[Pane]) -> Vec<String> {
    plan::plan(records, panes, "boot-2", ours)
        .into_iter()
        .map(|record| record.pane)
        .collect()
}

// --- Which records qualify -------------------------------------------------

#[test]
fn a_chat_seen_under_an_earlier_boot_is_continued_in_its_free_pane() {
    let records = vec![(record(ONE, "w1:p1"), Some("boot-1".to_owned()))];
    assert_eq!(planned(records, &[pane("w1:p1", false)]), ["w1:p1"]);
}

#[test]
fn a_chat_that_died_under_the_running_daemon_is_left_alone() {
    // Seen live under the current boot: the daemon did not restart, so the
    // pane was not restored and its chat ended some other way.
    let records = vec![(record(ONE, "w1:p1"), Some("boot-2".to_owned()))];
    assert!(planned(records, &[pane("w1:p1", false)]).is_empty());
    // Never seen live by any window: nothing says which boot it ran under.
    let records = vec![(record(ONE, "w1:p1"), None)];
    assert!(planned(records, &[pane("w1:p1", false)]).is_empty());
}

#[test]
fn only_a_free_pane_of_this_daemon_is_typed_into() {
    let seen = || Some("boot-1".to_owned());
    // The pane already runs an agent or a chat.
    assert!(planned(vec![(record(ONE, "w1:p1"), seen())], &[pane("w1:p1", true)]).is_empty());
    // The pane is gone.
    assert!(
        planned(
            vec![(record(ONE, "w1:p1"), seen())],
            &[pane("w1:p2", false)]
        )
        .is_empty()
    );
    // Another Herdr session's pane with the same id.
    let mut other = record(ONE, "w1:p1");
    other.socket = PathBuf::from("/home/me/.config/herdr/sessions/work/herdr.sock");
    assert!(planned(vec![(other, seen())], &[pane("w1:p1", false)]).is_empty());
}

#[test]
fn two_records_for_one_pane_continue_neither() {
    let seen = || Some("boot-1".to_owned());
    let records = vec![
        (record(ONE, "w1:p1"), seen()),
        (record(TWO, "w1:p1"), seen()),
    ];
    assert!(planned(records, &[pane("w1:p1", false)]).is_empty());
}

#[test]
fn records_whose_pane_is_gone_are_orphans_unless_live_now_or_never_seen() {
    let records = vec![
        (record(ONE, "w1:p9"), Some("boot-1".to_owned())),
        (record(TWO, "w1:p1"), Some("boot-1".to_owned())),
    ];
    let panes = [pane("w1:p1", false)];
    let orphans: Vec<&EndpointId> = plan::orphans(&records, &panes, "boot-2", ours).collect();
    assert_eq!(orphans, [&endpoint(ONE)]);
    // Seen under this boot, or never seen (its pane may be newer than the
    // snapshot), a chat whose pane is not listed stays.
    let live = vec![(record(ONE, "w1:p9"), Some("boot-2".to_owned()))];
    assert_eq!(plan::orphans(&live, &panes, "boot-2", ours).count(), 0);
    let unseen = vec![(record(ONE, "w1:p9"), None)];
    assert_eq!(plan::orphans(&unseen, &panes, "boot-2", ours).count(), 0);
    // Another daemon's record is not this one's to remove.
    let mut other = record(ONE, "w1:p9");
    other.socket = PathBuf::from("/elsewhere/herdr.sock");
    let other = vec![(other, Some("boot-1".to_owned()))];
    assert_eq!(plan::orphans(&other, &panes, "boot-2", ours).count(), 0);
}

#[test]
fn panes_are_occupied_by_a_reported_agent_or_a_chat_token() {
    let mut snapshot: ClientShellSnapshot = crate::sidebar::layout_tests::snapshot(1);
    let template = snapshot.agents[0].clone();
    let shell_pane = |id: &str| ClientShellPane {
        pane_id: id.to_owned(),
        workspace_id: "w0".to_owned(),
        tab_id: "t0".to_owned(),
        label: None,
        cwd: Some("/work".to_owned()),
        foreground_cwd: None,
        focused: false,
        right_click_passthrough: false,
    };
    snapshot.panes = vec![shell_pane("agent"), shell_pane("chat"), shell_pane("shell")];
    let agent = |pane: &str, name: Option<&str>, tokens: Vec<(String, String)>| {
        let mut agent = template.clone();
        agent.pane_id = pane.to_owned();
        agent.agent = name.map(str::to_owned);
        agent.tokens = tokens;
        agent
    };
    snapshot.agents = vec![
        agent("agent", Some("codex"), Vec::new()),
        agent(
            "chat",
            None,
            vec![(herdr_chat_protocol::TOKEN.to_owned(), ONE.to_owned())],
        ),
        agent("shell", None, Vec::new()),
    ];
    let panes = plan::panes(&snapshot);
    let occupied: Vec<(&str, bool)> = panes
        .iter()
        .map(|pane| (pane.id.as_str(), pane.occupied))
        .collect();
    assert_eq!(
        occupied,
        [("agent", true), ("chat", true), ("shell", false)]
    );
    assert_eq!(panes[2].cwd.as_deref(), Some("/work"));
    assert!(plan::free(&panes, "shell"));
    assert!(!plan::free(&panes, "chat"));
    assert!(!plan::free(&panes, "missing"));
}

// --- The idle shell --------------------------------------------------------

fn info(value: serde_json::Value) -> ProcessInfo {
    serde_json::from_value(value).unwrap()
}

/// Paths are already canonical in these fixtures.
fn same(path: &str) -> Option<PathBuf> {
    Some(PathBuf::from(path))
}

#[test]
fn only_the_shell_itself_in_the_foreground_in_the_folder_is_idle() {
    let cwd = Path::new("/work");
    let idle = info(json!({
        "shell_pid": 10, "foreground_process_group_id": 10,
        "foreground_processes": [{"pid": 10, "argv": ["zsh"], "cwd": "/work"}],
    }));
    assert!(plan::idle(&idle, None, cwd, same));
    // Another job in the foreground, such as an editor.
    let busy = info(json!({
        "shell_pid": 10, "foreground_process_group_id": 22,
        "foreground_processes": [{"pid": 22, "argv": ["vim"], "cwd": "/work"}],
    }));
    assert!(!plan::idle(&busy, Some("/work"), cwd, same));
    // An answer that cannot say is never idle.
    for unknown in [
        json!({"shell_pid": 10}),
        json!({"foreground_process_group_id": 10}),
        json!({}),
    ] {
        assert!(!plan::idle(&info(unknown), Some("/work"), cwd, same));
    }
    // The shell moved to another folder.
    let elsewhere = info(json!({
        "shell_pid": 10, "foreground_process_group_id": 10,
        "foreground_processes": [{"pid": 10, "argv": ["zsh"], "cwd": "/tmp"}],
    }));
    assert!(!plan::idle(&elsewhere, Some("/work"), cwd, same));
}

#[test]
fn the_snapshot_folder_stands_in_when_herdr_lists_no_shell_folder() {
    let cwd = Path::new("/private/work");
    let quiet = info(json!({"shell_pid": 10, "foreground_process_group_id": 10}));
    // Compared after canonicalization, as `/work` may be a link.
    let canonical = |path: &str| (path == "/work").then(|| PathBuf::from("/private/work"));
    assert!(plan::idle(&quiet, Some("/work"), cwd, canonical));
    assert!(!plan::idle(&quiet, None, cwd, canonical));
    // A folder that no longer exists does not compare equal.
    assert!(!plan::idle(&quiet, Some("/gone"), cwd, canonical));
}

// --- Scheduling ------------------------------------------------------------

#[test]
fn the_pass_runs_at_once_then_on_its_interval_up_to_its_count() {
    let now = Instant::now();
    let boot = scope("boot-2", 1);
    assert_eq!(plan::pass_due(None, None, &boot, now), Some(1));
    let ran = (boot.clone(), 1);
    let next = Some(now + PASS_INTERVAL);
    assert_eq!(plan::pass_due(Some(&ran), next, &boot, now), None);
    assert_eq!(
        plan::pass_due(Some(&ran), next, &boot, now + PASS_INTERVAL),
        Some(2)
    );
    let done = (boot.clone(), PASSES);
    assert_eq!(
        plan::pass_due(Some(&done), None, &boot, now + PASS_INTERVAL * 100),
        None
    );
    // A reconnect or a new boot starts over at once, whatever was due.
    assert_eq!(
        plan::pass_due(Some(&done), next, &scope("boot-2", 2), now),
        Some(1)
    );
    assert_eq!(
        plan::pass_due(Some(&done), next, &scope("boot-3", 1), now),
        Some(1)
    );
}

// --- Watching live chats end -----------------------------------------------

#[test]
fn a_chat_missing_long_enough_under_one_scope_ended() {
    let now = Instant::now();
    let boot = scope("boot-1", 1);
    let mut watched = Watched::default();
    assert!(watched.observe(&boot, &[endpoint(ONE)], now).is_empty());
    // Briefly missing, then back: not ended, and the clock starts over.
    assert!(
        watched
            .observe(&boot, &[], now + ENDED_AFTER / 2)
            .is_empty()
    );
    assert!(
        watched
            .observe(&boot, &[endpoint(ONE)], now + ENDED_AFTER)
            .is_empty()
    );
    let gone = now + ENDED_AFTER * 2;
    assert!(watched.observe(&boot, &[], gone).is_empty());
    assert_eq!(
        watched.observe(&boot, &[], gone + ENDED_AFTER),
        [endpoint(ONE)]
    );
    // Reported once.
    assert!(
        watched
            .observe(&boot, &[], gone + ENDED_AFTER * 3)
            .is_empty()
    );
}

#[test]
fn a_restart_or_reconnect_never_ends_a_watched_chat() {
    let now = Instant::now();
    let mut watched = Watched::default();
    assert!(
        watched
            .observe(&scope("boot-1", 1), &[endpoint(ONE)], now)
            .is_empty()
    );
    assert!(watched.observe(&scope("boot-1", 1), &[], now).is_empty());
    // The daemon restarted: the chat is missing because its pane was
    // restored, which is what continuing it is for.
    assert!(
        watched
            .observe(&scope("boot-2", 1), &[], now + ENDED_AFTER * 10)
            .is_empty()
    );
    // Nor does a reconnect to the same boot.
    assert!(
        watched
            .observe(&scope("boot-2", 1), &[endpoint(TWO)], now)
            .is_empty()
    );
    assert!(
        watched
            .observe(&scope("boot-2", 2), &[], now + ENDED_AFTER * 10)
            .is_empty()
    );
}

// --- The pass and its claim ------------------------------------------------

/// A restore directory holding `records`, each seen under `boot-1`.
fn directory(records: &[Record]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for record in records {
        record.write(dir.path()).unwrap();
        herdr_chat_protocol::restore::write_seen(dir.path(), &record.endpoint, "boot-1").unwrap();
    }
    dir
}

fn run(
    dir: &Path,
    panes: &[Pane],
    program: &str,
    last: bool,
    listening: bool,
    idle: bool,
) -> Vec<String> {
    let probes = Probes {
        owns: ours,
        listening: |_: &EndpointId| listening,
        idle: |_: &Pane, _: &Path| idle,
    };
    plan::pass(dir, None, panes, "boot-2", Path::new(program), last, probes)
        .claimed
        .into_iter()
        .map(|(record, _)| record.pane)
        .collect()
}

fn listed(dir: &Path) -> usize {
    herdr_chat_protocol::restore::list(dir, None).records.len()
}

#[test]
fn a_restored_chat_is_claimed_once_however_many_passes_or_windows_run() {
    let dir = directory(&[record(ONE, "w1:p1")]);
    let panes = [pane("w1:p1", false)];
    let claimed = plan::pass(
        dir.path(),
        None,
        &panes,
        "boot-2",
        Path::new(PROGRAM),
        false,
        Probes {
            owns: ours,
            listening: |_: &EndpointId| false,
            idle: |_: &Pane, _: &Path| true,
        },
    )
    .claimed;
    assert_eq!(claimed.len(), 1);
    assert_eq!(
        claimed[0].1,
        plan::line(Path::new(PROGRAM), &claimed[0].0).unwrap()
    );
    // The next run, or another window's, finds nothing left to claim.
    assert!(run(dir.path(), &panes, PROGRAM, false, false, true).is_empty());
    assert!(run(dir.path(), &panes, PROGRAM, true, false, true).is_empty());
}

#[test]
fn a_record_that_fails_a_check_is_left_unclaimed_for_a_later_run() {
    let dir = directory(&[record(ONE, "w1:p1")]);
    let panes = [pane("w1:p1", false)];
    // Its program still answers: not a restored pane.
    assert!(run(dir.path(), &panes, PROGRAM, false, true, true).is_empty());
    // The shell is busy or elsewhere.
    assert!(run(dir.path(), &panes, PROGRAM, false, false, false).is_empty());
    // No safe line can be made with this program path.
    assert!(
        run(
            dir.path(),
            &panes,
            "/odd'path/herdr-acp",
            false,
            false,
            true
        )
        .is_empty()
    );
    assert_eq!(listed(dir.path()), 1);
    // Once the shell is idle, a later run continues it.
    assert_eq!(
        run(dir.path(), &panes, PROGRAM, false, false, true),
        ["w1:p1"]
    );
}

#[test]
fn only_the_last_run_removes_records_whose_pane_is_gone() {
    let dir = directory(&[record(ONE, "w1:p9"), record(TWO, "w1:p1")]);
    let panes = [pane("w1:p1", true)];
    assert!(run(dir.path(), &panes, PROGRAM, false, false, true).is_empty());
    assert_eq!(listed(dir.path()), 2);
    assert!(run(dir.path(), &panes, PROGRAM, true, false, true).is_empty());
    let left = herdr_chat_protocol::restore::list(dir.path(), None).records;
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].0.pane, "w1:p1");
}

// --- The typed line --------------------------------------------------------

#[test]
fn the_typed_line_resumes_the_exact_session_and_quotes_each_word() {
    let program = Path::new(PROGRAM);
    let mut chat = record(ONE, "w1:p1");
    assert_eq!(
        plan::line(program, &chat).unwrap(),
        " sh -c 'exec \"$0\" \"$@\"' '/Applications/Herdr.app/Contents/MacOS/herdr-acp' \
         '--resume' '0c6f0c39-3c1e-4b7e-9a51-6d1b8e2d7f10' 'claude'"
    );
    chat.adapter = true;
    chat.agent = Agent::Codex;
    assert!(
        plan::line(program, &chat)
            .unwrap()
            .ends_with("'--resume' '0c6f0c39-3c1e-4b7e-9a51-6d1b8e2d7f10' '--acp' 'codex'")
    );
    // Nothing that could break out of its quotes is typed.
    assert_eq!(plan::line(Path::new("/odd'path/herdr-acp"), &chat), None);
    assert_eq!(plan::line(Path::new("/odd\npath/herdr-acp"), &chat), None);
    for session in ["x'; rm -rf ~; '", "-x", "", "a b"] {
        chat.session = session.to_owned();
        assert_eq!(plan::line(program, &chat), None, "{session:?}");
    }
}

// --- Review fixes: ownership, delivery, rotation, abandoned claims ---------

#[test]
fn another_sessions_record_for_the_same_pane_id_never_blocks_this_one() {
    let seen = || Some("boot-1".to_owned());
    // Two Herdr sessions on one machine each have a `w1:p1`.
    let mut theirs = record(TWO, "w1:p1");
    theirs.socket = PathBuf::from("/home/me/.config/herdr/sessions/work/herdr.sock");
    let records = vec![(record(ONE, "w1:p1"), seen()), (theirs, seen())];
    assert_eq!(planned(records, &[pane("w1:p1", false)]), ["w1:p1"]);
}

#[test]
fn a_chat_live_in_a_pane_replaces_older_records_naming_it() {
    let seen = || Some("boot-1".to_owned());
    let mut theirs = record(TWO, "w1:p1");
    theirs.socket = PathBuf::from("/elsewhere/herdr.sock");
    let records = vec![
        (record(ONE, "w1:p1"), seen()),
        (record(TWO, "w1:p2"), seen()),
        (theirs, seen()),
    ];
    let new: EndpointId = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".parse().unwrap();
    let live = [(new.clone(), "w1:p1".to_owned())];
    let replaced: Vec<&EndpointId> = plan::superseded(&records, &live, ours).collect();
    assert_eq!(replaced, [&endpoint(ONE)]);
    // The live chat's own record is never replaced.
    let own = vec![(record(ONE, "w1:p1"), seen())];
    let live = [(endpoint(ONE), "w1:p1".to_owned())];
    assert_eq!(plan::superseded(&own, &live, ours).count(), 0);
}

fn typed(endpoint: &str, pane: &str, scope: &Scope, since: Instant) -> Typed {
    Typed {
        endpoint: endpoint.parse().unwrap(),
        pane: pane.to_owned(),
        scope: scope.clone(),
        since,
    }
}

#[test]
fn a_typed_line_is_settled_only_once_its_chat_shows() {
    let now = Instant::now();
    let boot = scope("boot-2", 1);
    let mut lines = vec![typed(ONE, "w1:p1", &boot, now)];
    // Queued but no chat yet: still held.
    let none = HashSet::new();
    assert_eq!(
        plan::settle(&mut lines, Some(&boot), &none, now),
        Settled::default()
    );
    assert_eq!(lines.len(), 1);
    // A chat shows in its pane: confirmed, and its record can go.
    let shown: HashSet<&str> = ["w1:p1"].into();
    let settled = plan::settle(&mut lines, Some(&boot), &shown, now);
    assert_eq!(settled.confirmed, [endpoint(ONE)]);
    assert!(lines.is_empty());
}

#[test]
fn a_line_that_may_never_have_arrived_gives_its_record_back() {
    let now = Instant::now();
    let boot = scope("boot-2", 1);
    let none = HashSet::new();
    let shown: HashSet<&str> = ["w1:p1"].into();
    // The connection dropped or was replaced, or the daemon restarted:
    // the queued line may have been refused, so the record is returned
    // even if some pane now shows a chat.
    for current in [None, Some(scope("boot-2", 2)), Some(scope("boot-3", 1))] {
        let mut lines = vec![typed(ONE, "w1:p1", &boot, now)];
        let settled = plan::settle(&mut lines, current.as_ref(), &shown, now);
        assert_eq!(settled.returned, [endpoint(ONE)], "{current:?}");
        assert!(settled.confirmed.is_empty());
    }
    // Nothing showed in time.
    let mut lines = vec![typed(ONE, "w1:p1", &boot, now)];
    let late = now + CONFIRM_WITHIN;
    assert_eq!(
        plan::settle(&mut lines, Some(&boot), &none, late).returned,
        [endpoint(ONE)]
    );
}

#[test]
fn a_pass_with_more_candidates_than_probes_reads_on_next_run() {
    let ids: Vec<String> = (0..MAX_PROBES + 2).map(|n| format!("{n:032x}")).collect();
    let records: Vec<Record> = ids
        .iter()
        .enumerate()
        .map(|(n, id)| record(id, &format!("w1:p{n}")))
        .collect();
    let dir = directory(&records);
    let panes: Vec<Pane> = (0..ids.len())
        .map(|n| pane(&format!("w1:p{n}"), false))
        .collect();
    let asked = std::cell::Cell::new(0);
    // Every shell is busy, so nothing is claimed; each run still asks
    // about at most MAX_PROBES panes and says where to read on.
    let busy = |after: Option<&EndpointId>| {
        plan::pass(
            dir.path(),
            after,
            &panes,
            "boot-2",
            Path::new(PROGRAM),
            false,
            Probes {
                owns: ours,
                listening: |_: &EndpointId| false,
                idle: |_: &Pane, _: &Path| {
                    asked.set(asked.get() + 1);
                    false
                },
            },
        )
    };
    let first = busy(None);
    assert_eq!(asked.get(), MAX_PROBES);
    assert_eq!(first.next.as_ref(), Some(&endpoint(&ids[MAX_PROBES - 1])));
    // The next run starts with the ones the first never asked about.
    let claimed = plan::pass(
        dir.path(),
        first.next.as_ref(),
        &panes,
        "boot-2",
        Path::new(PROGRAM),
        false,
        Probes {
            owns: ours,
            listening: |_: &EndpointId| false,
            idle: |_: &Pane, _: &Path| true,
        },
    )
    .claimed;
    let first_two: Vec<&str> = claimed
        .iter()
        .take(2)
        .map(|(record, _)| record.endpoint.as_str())
        .collect();
    assert_eq!(
        first_two,
        [ids[MAX_PROBES].as_str(), ids[MAX_PROBES + 1].as_str()]
    );
}

#[test]
fn every_record_is_read_once_however_many_there_are() {
    let total = herdr_chat_protocol::restore::MAX_RECORDS + 5;
    let records: Vec<Record> = (0..total)
        .map(|n| record(&format!("{n:032x}"), &format!("w1:p{n}")))
        .collect();
    let dir = directory(&records);
    let read = plan::every_record(dir.path());
    assert_eq!(read.len(), total);
    let unique: HashSet<&str> = read
        .iter()
        .map(|(record, _)| record.endpoint.as_str())
        .collect();
    assert_eq!(unique.len(), total);
}

#[test]
fn a_pass_whose_window_is_gone_gives_its_claims_back() {
    let dir = directory(&[record(ONE, "w1:p1")]);
    let path = dir.path().to_owned();
    let (release, wait) = mpsc::channel::<()>();
    let receiver = spawn(path.clone(), move |dir| {
        let claimed = plan::pass(
            dir,
            None,
            &[pane("w1:p1", false)],
            "boot-2",
            Path::new(PROGRAM),
            false,
            Probes {
                owns: ours,
                listening: |_: &EndpointId| false,
                idle: |_: &Pane, _: &Path| true,
            },
        );
        // Hold the result until the window has gone.
        let _ = wait.recv();
        Outcome::Claimed {
            at: Instant::now(),
            claimed: claimed.claimed,
            next: None,
            scope: scope("boot-2", 1),
        }
    })
    .unwrap();
    wait_until(|| listed(&path) == 0);
    drop(receiver);
    release.send(()).unwrap();
    wait_until(|| listed(&path) == 1);
}

#[test]
fn every_live_chat_counts_when_judging_which_ended() {
    let count = MAX_STAMPS + 4;
    let mut snapshot: ClientShellSnapshot = crate::sidebar::layout_tests::snapshot(1);
    let template = snapshot.agents[0].clone();
    let ids: Vec<String> = (0..count).map(|n| format!("{n:032x}")).collect();
    snapshot.agents = ids
        .iter()
        .enumerate()
        .map(|(n, id)| {
            let mut agent = template.clone();
            agent.pane_id = format!("w1:p{n}");
            agent.tokens = vec![(herdr_chat_protocol::TOKEN.to_owned(), id.clone())];
            agent
        })
        .collect();
    let live = plan::live_chats(&snapshot);
    assert_eq!(
        live.len(),
        count,
        "no chat is left out of the snapshot's list"
    );

    // Watched against the whole list, the chat past the stamping batch is
    // never taken for ended, however long it runs.
    let boot = scope("boot-1", 1);
    let endpoints: Vec<EndpointId> = live.iter().map(|(endpoint, _)| endpoint.clone()).collect();
    let mut watched = Watched::default();
    let now = Instant::now();
    for tick in 0..4 {
        let at = now + ENDED_AFTER * tick;
        assert!(watched.observe(&boot, &endpoints, at).is_empty());
    }

    // Stamping takes a bounded batch, and the next batch the rest.
    let first = plan::to_stamp(live.clone(), &HashSet::new());
    assert_eq!(first.len(), MAX_STAMPS);
    let done: HashSet<EndpointId> = first.into_iter().map(|(endpoint, _)| endpoint).collect();
    let rest = plan::to_stamp(live, &done);
    assert_eq!(rest.len(), count - MAX_STAMPS);
}
