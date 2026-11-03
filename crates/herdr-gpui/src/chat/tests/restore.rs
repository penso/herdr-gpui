//! Continuing chats in panes Herdr restored: which records qualify, and the
//! line typed into the restored shell.

use super::*;
use crate::chat::restore::{self, Pane};
use herdr_chat_protocol::{Agent, EndpointId, restore::Record};
use herdr_client::protocol::{ClientShellPane, ClientShellSnapshot};
use std::path::Path;

const ONE: &str = "0123456789abcdef0123456789abcdef";
const TWO: &str = "fedcba9876543210fedcba9876543210";
const SOCKET: &str = "/home/me/.config/herdr/herdr.sock";

fn record(endpoint: &str, pane: &str) -> Record {
    Record {
        adapter: false,
        agent: Agent::Claude,
        cwd: PathBuf::from("/work"),
        endpoint: endpoint.parse().unwrap(),
        pane: pane.to_owned(),
        session: "0c6f0c39-3c1e-4b7e-9a51-6d1b8e2d7f10".to_owned(),
        socket: PathBuf::from(SOCKET),
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
    socket == Path::new(SOCKET)
}

fn planned(records: Vec<(Record, Option<String>)>, panes: &[Pane]) -> Vec<String> {
    restore::plan(records, panes, "boot-2", ours)
        .into_iter()
        .map(|record| record.pane)
        .collect()
}

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
fn records_whose_pane_is_gone_are_orphans_unless_live_now() {
    let records = vec![
        (record(ONE, "w1:p9"), Some("boot-1".to_owned())),
        (record(TWO, "w1:p1"), Some("boot-1".to_owned())),
    ];
    let panes = [pane("w1:p1", false)];
    let orphans: Vec<&EndpointId> = restore::orphans(&records, &panes, "boot-2", ours).collect();
    assert_eq!(orphans, [&ONE.parse::<EndpointId>().unwrap()]);
    // Seen under this boot, or never seen (its pane may be newer than the
    // snapshot), a chat whose pane is not listed stays.
    let live = vec![(record(ONE, "w1:p9"), Some("boot-2".to_owned()))];
    assert_eq!(restore::orphans(&live, &panes, "boot-2", ours).count(), 0);
    let unseen = vec![(record(ONE, "w1:p9"), None)];
    assert_eq!(restore::orphans(&unseen, &panes, "boot-2", ours).count(), 0);
    // Another daemon's record is not this one's to remove.
    let mut other = record(ONE, "w1:p9");
    other.socket = PathBuf::from("/elsewhere/herdr.sock");
    let other = vec![(other, Some("boot-1".to_owned()))];
    assert_eq!(restore::orphans(&other, &panes, "boot-2", ours).count(), 0);
}

#[test]
fn the_typed_line_resumes_the_exact_session_and_quotes_each_word() {
    let program = Path::new("/Applications/Herdr.app/Contents/MacOS/herdr-acp");
    let mut chat = record(ONE, "w1:p1");
    assert_eq!(
        restore::line(program, &chat).unwrap(),
        " sh -c 'exec \"$0\" \"$@\"' '/Applications/Herdr.app/Contents/MacOS/herdr-acp' \
         '--resume' '0c6f0c39-3c1e-4b7e-9a51-6d1b8e2d7f10' 'claude'"
    );
    chat.adapter = true;
    chat.agent = Agent::Codex;
    assert!(
        restore::line(program, &chat)
            .unwrap()
            .ends_with("'--resume' '0c6f0c39-3c1e-4b7e-9a51-6d1b8e2d7f10' '--acp' 'codex'")
    );
    // Nothing that could break out of its quotes is typed.
    assert_eq!(restore::line(Path::new("/odd'path/herdr-acp"), &chat), None);
    assert_eq!(
        restore::line(Path::new("/odd\npath/herdr-acp"), &chat),
        None
    );
    chat.session = "x'; rm -rf ~; '".to_owned();
    assert_eq!(restore::line(program, &chat), None);
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
    let panes = restore::panes(&snapshot);
    let occupied: Vec<(&str, bool)> = panes
        .iter()
        .map(|pane| (pane.id.as_str(), pane.occupied))
        .collect();
    assert_eq!(
        occupied,
        [("agent", true), ("chat", true), ("shell", false)]
    );
    assert_eq!(panes[2].cwd.as_deref(), Some("/work"));
}
