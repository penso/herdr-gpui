use super::*;
use crate::chat::{Chats, link::Link};
use herdr_chat_protocol::{Command, EndpointId};

const A: &str = "0123456789abcdef0123456789abcdef";
const B: &str = "fedcba9876543210fedcba9876543210";

fn target(pane: &str, endpoint: &str) -> Target {
    Target {
        existing: None,
        endpoint: endpoint.parse::<EndpointId>().unwrap(),
        pane_id: pane.into(),
        route: Route::Local,
    }
}

#[test]
fn an_observed_process_exit_immediately_restores_the_terminal() {
    let mut harness = Harness::new();
    let mut target = target("p1", A);
    target.existing = Some(existing::Binding {
        pane_id: "p1".into(),
        agent: herdr_chat_protocol::Agent::Codex,
        session_id: "01234567-89ab-4cde-8123-456789abcdef".into(),
        herdr_session: None,
        pid: 21,
        shell_pid: 20,
        started_ticks: 123,
    });
    harness.reconcile(vec![target]);
    let (mut stream, _) = harness.program.accept();
    send(&mut stream, &welcome());
    send(
        &mut stream,
        &snapshot(cursor(1, 0), vec![agent(1, "History")]),
    );
    harness.poll_until(|chats| chats.pane("p1").is_some_and(|pane| pane.available()));
    send(
        &mut stream,
        &ServerMessage::Bye {
            reason: ByeReason::Exiting,
        },
    );
    harness.poll_until(|chats| chats.pane("p1").is_some_and(|pane| !pane.attached()));
    assert!(!harness.chats.pane("p1").unwrap().available());
}

/// A window's chats, connecting every pane to one stand-in program.
struct Harness {
    chats: Chats,
    now: Instant,
    program: Program,
    sinces: Vec<Option<Cursor>>,
}

impl Harness {
    fn new() -> Self {
        Self {
            chats: Chats::default(),
            now: Instant::now(),
            program: Program::new(),
            sinces: Vec::new(),
        }
    }

    fn reconcile(&mut self, targets: Vec<Target>) {
        let path = self.program.path.clone();
        let sinces = &mut self.sinces;
        self.chats
            .reconcile(targets, self.now, &mut |_, since, resend| {
                sinces.push(since);
                Link::start(path.clone(), since, resend)
            });
    }

    /// Lets every retry come due.
    fn later(&mut self) {
        self.now += Duration::from_secs(60);
    }

    fn poll_until(&mut self, mut done: impl FnMut(&Chats) -> bool) {
        let now = self.now;
        let chats = &mut self.chats;
        wait_until(|| {
            chats.poll(now);
            done(chats)
        });
    }

    /// Attaches `p1` to a program that greets and sends `entries`.
    fn attach(&mut self, at: Cursor, entries: Vec<Entry>) -> Peer {
        self.reconcile(vec![target("p1", A)]);
        let (mut stream, _) = self.program.accept();
        send(&mut stream, &welcome());
        send(&mut stream, &snapshot(at, entries));
        self.poll_until(|chats| chats.pane("p1").is_some_and(|pane| pane.attached()));
        stream
    }
}

fn text(chats: &Chats) -> Vec<String> {
    chats
        .pane("p1")
        .unwrap()
        .session
        .transcript()
        .entries
        .iter()
        .filter_map(|entry| entry.kind.text().map(str::to_owned))
        .collect()
}

#[test]
fn an_attached_chat_shows_until_turned_back_to_its_terminal() {
    let mut harness = Harness::new();
    assert!(!harness.chats.shown("p1"));
    let _stream = harness.attach(cursor(1, 0), vec![agent(1, "hi")]);
    assert!(harness.chats.shown("p1"));
    assert!(harness.chats.set_raw("p1", true));
    assert!(!harness.chats.shown("p1"));
    assert!(harness.chats.raw("p1"));
    assert!(harness.chats.set_raw("p1", false));
    assert!(harness.chats.shown("p1"));
    assert!(!harness.chats.set_raw("p9", true));
    assert_eq!(harness.chats.pane("p1").unwrap().list.item_count(), 1);
}

#[test]
fn a_dropped_connection_resumes_from_its_cursor() {
    let mut harness = Harness::new();
    let mut stream = harness.attach(cursor(1, 0), vec![agent(1, "a")]);
    send(&mut stream, &change(append(1, "b"), cursor(1, 1)));
    drop(stream);
    harness.poll_until(|chats| !chats.pane("p1").unwrap().attached());
    // Still shown, with what it had, while it reconnects.
    assert!(harness.chats.shown("p1"));
    assert_eq!(text(&harness.chats), ["ab"]);
    let generation = harness.chats.pane("p1").unwrap().generation();
    // Nothing reconnects before the retry is due.
    harness.reconcile(vec![target("p1", A)]);
    assert_eq!(harness.sinces.len(), 1);
    harness.later();
    harness.reconcile(vec![target("p1", A)]);
    let (_stream, greeting) = harness.program.accept();
    assert_eq!(greeting, hello(Some(cursor(1, 1))));
    assert_eq!(
        harness.chats.pane("p1").unwrap().generation(),
        generation + 1
    );
}

#[test]
fn nothing_after_a_change_that_does_not_fit_is_applied() {
    let mut harness = Harness::new();
    let mut stream = harness.attach(cursor(1, 0), vec![agent(1, "a")]);
    // A gap, then what would follow it: the connection is dropped at the
    // gap, and what it read after is never applied.
    send(&mut stream, &change(append(1, "x"), cursor(1, 2)));
    send(&mut stream, &change(append(1, "y"), cursor(1, 3)));
    harness.poll_until(|chats| !chats.pane("p1").unwrap().attached());
    assert_eq!(text(&harness.chats), ["a"]);
    // The next connection starts over from a snapshot, at once.
    harness.reconcile(vec![target("p1", A)]);
    let (mut stream, greeting) = harness.program.accept();
    assert_eq!(greeting, hello(None));
    send(&mut stream, &welcome());
    send(&mut stream, &snapshot(cursor(1, 3), vec![agent(1, "axy")]));
    harness.poll_until(|chats| chats.pane("p1").unwrap().attached());
    assert_eq!(text(&harness.chats), ["axy"]);
}

#[test]
fn a_new_program_in_the_same_pane_starts_a_new_chat() {
    let mut harness = Harness::new();
    let mut old = harness.attach(cursor(1, 0), vec![agent(1, "old")]);
    harness.chats.set_raw("p1", true);
    harness.reconcile(vec![target("p1", B)]);
    let pane = harness.chats.pane("p1").unwrap();
    assert_eq!(pane.target, target("p1", B));
    assert!(!pane.session.synced());
    assert!(!harness.chats.raw("p1"));
    // The old program's connection was closed with its chat.
    assert_eq!(read(&mut old), None);
    let (_new, greeting) = harness.program.accept();
    assert_eq!(greeting, hello(None));
}

#[test]
fn a_pane_that_goes_away_is_detached() {
    let mut harness = Harness::new();
    let mut stream = harness.attach(cursor(1, 0), Vec::new());
    harness.reconcile(Vec::new());
    assert!(harness.chats.pane("p1").is_none());
    assert_eq!(read(&mut stream), None);
}

#[test]
fn a_program_that_stays_away_gives_the_pane_back_to_its_terminal() {
    let mut harness = Harness::new();
    let stream = harness.attach(cursor(1, 0), Vec::new());
    drop(stream);
    harness.program.stop();
    harness.poll_until(|chats| !chats.pane("p1").unwrap().attached());
    for _ in 1..MAX_FAILURES {
        assert!(harness.chats.shown("p1"));
        harness.later();
        harness.reconcile(vec![target("p1", A)]);
        harness.poll_until(|chats| !chats.pane("p1").unwrap().has_link());
    }
    assert!(!harness.chats.shown("p1"));
    assert!(!harness.chats.pane("p1").unwrap().available());
}

#[test]
fn another_protocol_version_never_reconnects() {
    let mut harness = Harness::new();
    harness.reconcile(vec![target("p1", A)]);
    let (mut stream, _) = harness.program.accept();
    send(
        &mut stream,
        &ServerMessage::Welcome {
            agent: "x".into(),
            pane: None,
            version: VERSION + 1,
        },
    );
    harness.poll_until(|chats| !chats.pane("p1").unwrap().has_link());
    assert_eq!(read(&mut stream), None);
    harness.later();
    harness.reconcile(vec![target("p1", A)]);
    assert_eq!(harness.sinces.len(), 1);
}

#[test]
fn a_command_sent_while_away_goes_out_on_reconnect() {
    let mut harness = Harness::new();
    let stream = harness.attach(cursor(1, 0), Vec::new());
    drop(stream);
    harness.poll_until(|chats| !chats.pane("p1").unwrap().attached());
    let pane = harness.chats.pane_mut("p1").unwrap();
    pane.send(Command::Prompt {
        text: "hi".into(),
        attachments: Vec::new(),
    })
    .unwrap();
    harness.later();
    harness.reconcile(vec![target("p1", A)]);
    let (mut stream, _) = harness.program.accept();
    let Some(ClientMessage::Command { command, .. }) = read(&mut stream) else {
        panic!("the prompt");
    };
    assert_eq!(
        command,
        Command::Prompt {
            text: "hi".into(),
            attachments: Vec::new(),
        }
    );
}
