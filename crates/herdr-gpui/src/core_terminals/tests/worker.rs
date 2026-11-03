//! The worker runs any `herdr_backend::Backend`; a scripted one stands in
//! for herdr-core here.

use crate::core_terminals::worker::{self, Command};
use herdr_backend::{
    Backend, Changes, ClientPaneInputEvent, FrameData, Generation, Role, Size, Spawn,
    SurfaceUpdate, Terminal, TerminalId, TerminalInfo,
};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Debug, thiserror::Error)]
#[error("scripted failure")]
struct Failure;

#[derive(Clone, Default)]
struct Scripted {
    /// What the terminal received, in order.
    received: Arc<Mutex<Vec<Command>>>,
    /// Updates for the terminal to hand out, one batch per `next`.
    updates: Arc<Mutex<Vec<Vec<SurfaceUpdate>>>>,
    fail_attach: bool,
}

struct ScriptedTerminal(Scripted);

struct NoChanges;

impl Changes for NoChanges {
    type Error = Failure;

    fn next(&mut self, _: Duration) -> Result<Vec<herdr_backend::Change>, Failure> {
        Ok(Vec::new())
    }
}

impl Backend for Scripted {
    type Changes = NoChanges;
    type Error = Failure;
    type Terminal = ScriptedTerminal;

    fn generation(&self) -> Generation {
        Generation::Herdr2
    }

    fn terminals(&self) -> Result<Vec<TerminalInfo>, Failure> {
        Ok(Vec::new())
    }

    fn watch(&self) -> Result<(Vec<TerminalInfo>, NoChanges), Failure> {
        Ok((Vec::new(), NoChanges))
    }

    fn attach(&self, _: &TerminalId, size: Size, _: Role) -> Result<ScriptedTerminal, Failure> {
        if self.fail_attach {
            return Err(Failure);
        }
        self.received.lock().unwrap().push(Command::Resize(size));
        Ok(ScriptedTerminal(self.clone()))
    }

    fn spawn(&self, _: &Spawn) -> Result<TerminalInfo, Failure> {
        Err(Failure)
    }

    fn kill(&self, _: &TerminalId) -> Result<(), Failure> {
        Ok(())
    }
}

impl Terminal for ScriptedTerminal {
    type Error = Failure;

    fn input(&mut self, input: &ClientPaneInputEvent) -> Result<(), Failure> {
        self.0
            .received
            .lock()
            .unwrap()
            .push(Command::Input(input.clone()));
        Ok(())
    }

    fn resize(&mut self, size: Size) -> Result<(), Failure> {
        self.0.received.lock().unwrap().push(Command::Resize(size));
        Ok(())
    }

    fn focus(&mut self, focused: bool) -> Result<(), Failure> {
        self.0
            .received
            .lock()
            .unwrap()
            .push(Command::Focus(focused));
        Ok(())
    }

    fn next(&mut self, timeout: Duration) -> Result<Vec<SurfaceUpdate>, Failure> {
        let batch = self.0.updates.lock().unwrap().pop();
        if batch.is_none() {
            std::thread::sleep(timeout);
        }
        Ok(batch.unwrap_or_default())
    }
}

fn until<T>(mut ready: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(value) = ready() {
            return value;
        }
        assert!(Instant::now() < deadline, "timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn size(cols: u16, rows: u16) -> Size {
    Size::new(cols, rows).unwrap()
}

#[test]
fn commands_reach_the_terminal_in_order_and_screens_come_back() {
    let backend = Scripted::default();
    let frame = FrameData {
        cells: Vec::new(),
        width: 0,
        height: 0,
        cursor: None,
        hyperlinks: Vec::new(),
        graphics: Vec::new(),
    };
    backend.updates.lock().unwrap().push(vec![
        SurfaceUpdate::Frame(frame.clone()),
        SurfaceUpdate::Title("sh".into()),
    ]);
    let link = worker::spawn(
        backend.clone(),
        TerminalId::new("t1").unwrap(),
        size(80, 24),
        Role::Active,
    )
    .unwrap();
    let screen = until(|| link.mailbox.take());
    assert_eq!(screen.frame.as_deref(), Some(&frame));
    assert_eq!(screen.title.as_deref(), Some("sh"));
    assert!(link.mailbox.take().is_none(), "a screen is taken once");

    link.send(Command::Focus(true));
    link.send(Command::Input(ClientPaneInputEvent::TextCommit(
        "ls".into(),
    )));
    link.send(Command::Resize(size(100, 30)));
    let received = until(|| {
        let received = backend.received.lock().unwrap().clone();
        (received.len() == 4).then_some(received)
    });
    assert_eq!(
        received,
        [
            Command::Resize(size(80, 24)),
            Command::Focus(true),
            Command::Input(ClientPaneInputEvent::TextCommit("ls".into())),
            Command::Resize(size(100, 30)),
        ]
    );
}

#[test]
fn a_terminal_that_cannot_attach_says_why() {
    let backend = Scripted {
        fail_attach: true,
        ..Scripted::default()
    };
    let link = worker::spawn(
        backend,
        TerminalId::new("t1").unwrap(),
        size(80, 24),
        Role::Active,
    )
    .unwrap();
    let screen = until(|| link.mailbox.take());
    assert_eq!(screen.failure.as_deref(), Some("scripted failure"));
}

#[test]
fn dropping_the_link_stops_the_worker() {
    let backend = Scripted::default();
    let link = worker::spawn(
        backend.clone(),
        TerminalId::new("t1").unwrap(),
        size(80, 24),
        Role::Active,
    )
    .unwrap();
    until(|| (!backend.received.lock().unwrap().is_empty()).then_some(()));
    drop(link);
    // The worker's clones of the backend go when its thread ends.
    until(|| (Arc::strong_count(&backend.received) == 1).then_some(()));
}

#[test]
fn an_ended_terminal_keeps_its_screen_and_stops_its_worker() {
    let backend = Scripted::default();
    backend
        .updates
        .lock()
        .unwrap()
        .push(vec![SurfaceUpdate::Ended(herdr_backend::Exit {
            code: Some(0),
        })]);
    let link = worker::spawn(
        backend.clone(),
        TerminalId::new("t1").unwrap(),
        size(80, 24),
        Role::Active,
    )
    .unwrap();
    let screen = until(|| link.mailbox.take());
    assert_eq!(screen.ended, Some(herdr_backend::Exit { code: Some(0) }));
    // The link is still held: the worker ended on its own.
    until(|| (Arc::strong_count(&backend.received) == 1).then_some(()));
    drop(link);
}
