use super::{MockCore, connected, terminal};
use crate::{CoreBackend, frame::Kind};
use herdr_backend::{
    AgentStatus, Backend, Change, Changes, ClientPaneInputEvent, Exit, KeyReporting, Role,
    RunState, Size, SurfaceUpdate, Terminal as _, TerminalId,
};
use serde_json::{Value, json};
use std::{thread, time::Duration};

const WAIT: Duration = Duration::from_secs(5);

fn backend() -> (CoreBackend, MockCore) {
    let (core, mock) = connected();
    (CoreBackend::new(core, 1000), mock)
}

fn line(value: Value) -> Vec<u8> {
    [serde_json::to_vec(&value).unwrap(), b"\n".to_vec()].concat()
}

fn event(seq: u64, kind: &str, address: &str, data: Value) -> Vec<u8> {
    line(
        json!({"method": "event", "params": {"seq": seq, "type": kind, "terminal": address, "data": data}}),
    )
}

/// Watches with `t1` live, its status working, at seq 10 on channel 3.
fn watching(backend: &CoreBackend, mock: &mut MockCore) -> crate::CoreChanges {
    let watch = thread::scope(|scope| {
        let watch = scope.spawn(|| backend.watch());
        let request = mock.request();
        assert_eq!(request["method"], "events.subscribe");
        let mut t1 = terminal("t1");
        t1["status"] = json!({"state": "working"});
        mock.answer(
            &request,
            json!({"channel": 3, "seq": 10, "terminals": [t1]}),
        );
        watch.join().unwrap()
    });
    let (snapshot, changes) = watch.unwrap();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(
        (snapshot[0].id.as_str(), snapshot[0].agent),
        ("t1", AgentStatus::Working)
    );
    changes
}

fn changes_after(changes: &mut crate::CoreChanges, count: usize) -> Vec<Change> {
    let mut all = Vec::new();
    for _ in 0..100 {
        all.extend(changes.next(Duration::from_millis(50)).unwrap());
        if all.len() >= count {
            break;
        }
    }
    all
}

#[test]
fn events_update_whole_records() {
    let (backend, mut mock) = backend();
    let mut changes = watching(&backend, &mut mock);
    let bytes = [
        event(11, "title", "t1", json!({"title": "vim"})),
        event(
            12,
            "status",
            "t1",
            json!({"status": {"state": "error"}, "reported": true}),
        ),
        event(13, "turn", "t1", json!({"turn": 2, "state": "done"})),
        event(14, "created", "t2", terminal("t2")),
        event(15, "removed", "t2", json!({})),
        event(
            16,
            "finished",
            "t1",
            json!({"reason": "exited", "exit_code": 4, "signal": null, "finished_ms": 1}),
        ),
    ]
    .concat();
    mock.send(3, Kind::Data, &bytes);
    let all = changes_after(&mut changes, 5);
    let Change::Upsert(titled) = &all[0] else {
        panic!("{all:?}")
    };
    assert_eq!(
        (titled.title.as_deref(), titled.agent),
        (Some("vim"), AgentStatus::Working)
    );
    // An error asks for the user as blocked does; the title stays.
    let Change::Upsert(errored) = &all[1] else {
        panic!("{all:?}")
    };
    assert_eq!(
        (errored.title.as_deref(), errored.agent),
        (Some("vim"), AgentStatus::Blocked)
    );
    // A turn changes nothing the app shows.
    assert!(matches!(&all[2], Change::Upsert(info) if info.id.as_str() == "t2"));
    assert_eq!(all[3], Change::Removed(TerminalId::new("t2").unwrap()));
    let Change::Upsert(finished) = &all[4] else {
        panic!("{all:?}")
    };
    assert_eq!(
        (finished.state, finished.agent),
        (
            RunState::Exited(Exit { code: Some(4) }),
            AgentStatus::Unknown
        )
    );
}

#[test]
fn a_missed_snapshot_replaces_everything_and_fences_older_events() {
    let (backend, mut mock) = backend();
    let mut changes = watching(&backend, &mut mock);
    let missed =
        line(json!({"method": "missed", "params": {"seq": 20, "terminals": [terminal("t5")]}}));
    let bytes = [
        missed,
        event(19, "title", "t5", json!({"title": "stale"})),
        event(21, "title", "t5", json!({"title": "new"})),
    ]
    .concat();
    mock.send(3, Kind::Data, &bytes);
    let all = changes_after(&mut changes, 2);
    assert!(
        matches!(&all[0], Change::Resync(terminals) if terminals.len() == 1 && terminals[0].id.as_str() == "t5")
    );
    assert!(matches!(&all[1], Change::Upsert(info) if info.title.as_deref() == Some("new")));
    assert_eq!(all.len(), 2);
}

#[test]
fn a_lost_kept_terminal_reads_as_lost() {
    let (backend, mut mock) = backend();
    let mut changes = watching(&backend, &mut mock);
    mock.send(
        3,
        Kind::Data,
        &event(
            11,
            "lost",
            "t1",
            json!({"reason": "killed", "exit_code": null, "signal": 9, "finished_ms": null}),
        ),
    );
    let all = changes_after(&mut changes, 1);
    assert!(matches!(&all[0], Change::Upsert(info) if info.state == RunState::Lost));
}

fn attached(backend: &CoreBackend, mock: &mut MockCore, role: Role) -> crate::CoreTerminal {
    thread::scope(|scope| {
        let attach = scope.spawn(|| {
            backend.attach(
                &TerminalId::new("t1").unwrap(),
                Size::new(20, 4).unwrap(),
                role,
            )
        });
        let request = mock.request();
        assert_eq!(
            request["params"]["observe"].as_bool().unwrap_or(false),
            role == Role::Observer
        );
        let mut t1 = terminal("t1");
        (t1["cols"], t1["rows"]) = (json!(20), json!(4));
        mock.answer(&request, json!({"channel": 5, "terminal": t1}));
        attach.join().unwrap()
    })
    .unwrap()
}

#[test]
fn an_attached_terminal_paints_cells_and_ends() {
    let (backend, mut mock) = backend();
    let mut terminal = attached(&backend, &mut mock, Role::Active);
    mock.send(5, Kind::Data, b"\x1b]2;work\x07hi");
    let updates = terminal.next(WAIT).unwrap();
    assert!(updates.contains(&SurfaceUpdate::Title("work".into())));
    let frame = updates.iter().find_map(|update| match update {
        SurfaceUpdate::Frame(frame) => Some(frame),
        _ => None,
    });
    let frame = frame.expect("a first full frame");
    assert_eq!((frame.width, frame.height), (20, 4));
    assert_eq!(
        (
            frame.cells[0].symbol.as_str(),
            frame.cells[1].symbol.as_str()
        ),
        ("h", "i")
    );

    terminal
        .input(&ClientPaneInputEvent::TextCommit("ls".into()))
        .unwrap();
    let typed = (0..4)
        .map(|_| mock.frame())
        .find(|frame| frame.kind == Kind::Data)
        .unwrap();
    assert_eq!((typed.channel, typed.payload.as_slice()), (5, &b"ls"[..]));

    mock.send(5, Kind::Close, br#"{"reason":"exited","exit_code":0}"#);
    let ended = (0..50)
        .flat_map(|_| terminal.next(Duration::from_millis(50)).unwrap())
        .find(|update| matches!(update, SurfaceUpdate::Ended(_)));
    assert_eq!(ended, Some(SurfaceUpdate::Ended(Exit { code: Some(0) })));
    assert_eq!(terminal.next(Duration::ZERO).unwrap(), []);
}

#[test]
fn an_observer_never_types_resizes_or_focuses() {
    let (backend, mut mock) = backend();
    let mut terminal = attached(&backend, &mut mock, Role::Observer);
    terminal
        .input(&ClientPaneInputEvent::TextCommit("rm -rf".into()))
        .unwrap();
    terminal.resize(Size::new(30, 5).unwrap()).unwrap();
    terminal.focus(true).unwrap();
    drop(terminal);
    // The first frame the client sends after attaching is its detach.
    let frame = mock.frame();
    assert_eq!((frame.channel, frame.kind), (5, Kind::Close));
}

#[test]
fn a_synchronized_update_that_never_ends_is_shown_in_time() {
    let (backend, mut mock) = backend();
    let mut terminal = attached(&backend, &mut mock, Role::Active);
    let _ = terminal.next(Duration::from_millis(20)).unwrap();
    mock.send(5, Kind::Data, b"\x1b[?2026hheld");
    let start = std::time::Instant::now();
    let shown = loop {
        let updates = terminal.next(Duration::from_millis(50)).unwrap();
        let text: String = updates
            .iter()
            .flat_map(|update| match update {
                SurfaceUpdate::Frame(frame) => frame.cells.clone(),
                SurfaceUpdate::Rows { rows, .. } => {
                    rows.iter().flat_map(|row| row.cells.clone()).collect()
                }
                _ => Vec::new(),
            })
            .map(|cell| cell.symbol)
            .collect();
        if text.contains("held") {
            break start.elapsed();
        }
        assert!(start.elapsed() < WAIT, "never shown");
    };
    assert!(
        shown >= Duration::from_millis(100),
        "shown before the update could end: {shown:?}"
    );
}

/// The full frame among `updates`, if any.
fn full_frame(updates: &[SurfaceUpdate]) -> Option<&herdr_backend::FrameData> {
    updates.iter().find_map(|update| match update {
        SurfaceUpdate::Frame(frame) => Some(frame),
        _ => None,
    })
}

fn row_text(frame: &herdr_backend::FrameData, row: u16) -> String {
    let width = usize::from(frame.width);
    frame.cells[usize::from(row) * width..][..width]
        .iter()
        .map(|cell| cell.symbol.as_str())
        .collect::<String>()
        .trim_end()
        .to_owned()
}

fn wheel(kind: herdr_backend::ClientMouseKind, lines: u16) -> ClientPaneInputEvent {
    ClientPaneInputEvent::Mouse {
        kind,
        position: herdr_backend::ClientMousePosition::Cell { column: 0, row: 0 },
        geometry: None,
        modifiers: 0,
        lines,
    }
}

#[test]
fn the_view_is_told_when_the_program_starts_reading_the_mouse() {
    let (backend, mut mock) = backend();
    let mut terminal = attached(&backend, &mut mock, Role::Active);
    mock.send(5, Kind::Data, b"hi");
    let first = terminal.next(WAIT).unwrap();
    assert!(first.contains(&SurfaceUpdate::MouseReporting(false)));
    mock.send(5, Kind::Data, b"\x1b[?1000h");
    let updates = terminal.next(WAIT).unwrap();
    assert!(updates.contains(&SurfaceUpdate::MouseReporting(true)));
    // Told once, not on every update.
    mock.send(5, Kind::Data, b"more");
    let later = terminal.next(WAIT).unwrap();
    assert!(
        !later
            .iter()
            .any(|update| matches!(update, SurfaceUpdate::MouseReporting(_)))
    );
}

#[test]
fn a_wheel_the_program_ignores_scrolls_history_and_typing_returns_live() {
    let (backend, mut mock) = backend();
    let mut terminal = attached(&backend, &mut mock, Role::Active);
    let lines: String = (0..10).map(|line| format!("line{line}\r\n")).collect();
    mock.send(5, Kind::Data, lines.as_bytes());
    let live = terminal.next(WAIT).unwrap();
    assert_eq!(row_text(full_frame(&live).unwrap(), 0), "line7");

    terminal
        .input(&wheel(herdr_backend::ClientMouseKind::ScrollUp, 3))
        .unwrap();
    let scrolled = terminal.next(Duration::from_millis(50)).unwrap();
    assert_eq!(row_text(full_frame(&scrolled).unwrap(), 0), "line4");

    terminal
        .input(&ClientPaneInputEvent::TextCommit("x".into()))
        .unwrap();
    let back = terminal.next(Duration::from_millis(50)).unwrap();
    assert_eq!(row_text(full_frame(&back).unwrap(), 0), "line7");
    // Only the typing reached the program; the wheel stayed in the view.
    let written = (0..4)
        .map(|_| mock.frame())
        .find(|frame| frame.kind == Kind::Data)
        .unwrap();
    assert_eq!(written.payload.as_slice(), b"x");
}

#[test]
fn an_observer_draws_at_the_size_the_active_viewer_set() {
    let (backend, mut mock) = backend();
    let mut observer = attached(&backend, &mut mock, Role::Observer);
    // Its own view's bounds do not size it.
    observer.resize(Size::new(8, 2).unwrap()).unwrap();
    mock.send(5, Kind::Data, b"x");
    let first = observer.next(WAIT).unwrap();
    assert!(first.iter().any(|update| matches!(update,
        SurfaceUpdate::Frame(frame) if (frame.width, frame.height) == (20, 4))));
    // The active viewer widened the terminal; the observer follows.
    thread::sleep(Duration::from_millis(600));
    let resized = thread::scope(|scope| {
        let next = scope.spawn(|| observer.next(Duration::from_millis(50)));
        let request = mock.request();
        assert_eq!(request["method"], "terminal.list");
        let mut t1 = terminal("t1");
        (t1["cols"], t1["rows"]) = (json!(30), json!(6));
        mock.answer(&request, json!({"terminals": [t1]}));
        next.join().unwrap().unwrap()
    });
    assert!(resized.iter().any(|update| matches!(update,
        SurfaceUpdate::Frame(frame) if (frame.width, frame.height) == (30, 6))));
}

#[test]
fn the_view_is_told_which_key_events_the_program_asks_for() {
    let (backend, mut mock) = backend();
    let mut terminal = attached(&backend, &mut mock, Role::Active);
    mock.send(5, Kind::Data, b"hi");
    let first = terminal.next(WAIT).unwrap();
    assert!(first.contains(&SurfaceUpdate::KeyReporting(KeyReporting::default())));
    // kitty: push event types (2) and all keys as escapes (8).
    mock.send(5, Kind::Data, b"\x1b[>10u");
    let updates = terminal.next(WAIT).unwrap();
    assert!(updates.contains(&SurfaceUpdate::KeyReporting(KeyReporting {
        every_key: true,
        releases: true,
    })));
    // Its releases encode as kitty release events.
    let release = ClientPaneInputEvent::Key {
        code: herdr_backend::ClientKeyCode::Char('a'),
        modifiers: 0,
        kind: herdr_backend::ClientKeyKind::Release,
        repeat_count: 1,
        shifted_codepoint: None,
        generated_text: None,
        tracks_release: true,
        physical_key_id: None,
        windows_record: None,
    };
    terminal.input(&release).unwrap();
    let typed = (0..4)
        .map(|_| mock.frame())
        .find(|frame| frame.kind == Kind::Data)
        .unwrap();
    assert_eq!(typed.payload, b"\x1b[97;1:3u");
}
