//! One live socket's views: what the phone subscribed to, each followed by
//! its own task that sends the view's state now and after every burst of
//! changes. Nothing here touches the socket; it reads parsed messages and
//! writes typed ones, so it is tested without a WebSocket.

use super::{
    home,
    model::{self, ChatSnapshot},
    wire::{ByeReason, ClientMessage, MAX_SCROLL_LINES, ServerMessage, Step, TermSize, View},
};
use crate::{
    control::DeviceId,
    hub::{Chat, Event},
    web::{Shared, now_ms, read_screen},
};
use herdr_chat_protocol::{Change, Cursor, EntryKind, Transcript};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::{
    sync::{broadcast::error::RecvError, mpsc, watch},
    task::{AbortHandle, JoinSet},
};

/// Views one socket follows at once.
pub(crate) const MAX_VIEWS: usize = 8;
/// How long list changes gather before one `home` goes out.
const HOME_COALESCE: Duration = Duration::from_millis(400);
/// How long a chat's changes gather before one `chat_changes` goes out.
const CHAT_COALESCE: Duration = Duration::from_millis(120);
/// Scrolls one terminal view holds before it reads them; more are dropped.
const SCROLL_QUEUE: usize = 8;
/// What the phone hears when it scrolls a terminal no attach holds.
const CANNOT_SCROLL: &str = "this terminal cannot scroll from the phone";
/// Failed screen reads in a row before the view reloads.
const SCREEN_FAILURES: u32 = 3;
/// How often the session is checked, so one that expires ends within this
/// long. A revocation ends it at once.
const ACCESS_CHECK: Duration = Duration::from_secs(30);

type Outbound = mpsc::Sender<ServerMessage>;

mod term_size;

/// The views a socket follows, and their tasks. Dropping it stops them.
#[derive(Default)]
pub(crate) struct Views {
    tasks: JoinSet<()>,
    running: HashMap<View, AbortHandle>,
    /// What each running terminal view takes besides its subscription,
    /// by pane.
    terms: HashMap<String, TermControl>,
}

/// The size a terminal view follows at, and the scrolls it forwards.
struct TermControl {
    size: watch::Sender<Option<TermSize>>,
    scrolls: mpsc::Sender<i32>,
}

impl Views {
    /// Whether `view` may start: it replaces itself, and a new one fits
    /// under [`MAX_VIEWS`] once finished views are forgotten.
    pub(crate) fn admits(&mut self, view: &View) -> bool {
        while self.tasks.try_join_next().is_some() {}
        self.running.retain(|_, task| !task.is_finished());
        self.running.contains_key(view) || self.running.len() < MAX_VIEWS
    }

    fn start(&mut self, view: View, task: impl Future<Output = ()> + Send + 'static) {
        self.stop(&view);
        let handle = self.tasks.spawn(task);
        self.running.insert(view, handle);
    }

    fn stop(&mut self, view: &View) {
        if let View::Term { pane } = view {
            self.terms.remove(pane);
        }
        if let Some(task) = self.running.remove(view) {
            task.abort();
        }
    }

    /// The controls of a running terminal view of `pane`.
    fn term(&self, pane: &str) -> Option<&TermControl> {
        let running = self
            .running
            .get(&View::Term {
                pane: pane.to_owned(),
            })
            .is_some_and(|task| !task.is_finished());
        self.terms
            .get(pane)
            .filter(|term| running && !term.size.is_closed())
    }

    /// Hands a running terminal view of `pane` its new size, which also
    /// sends its screen again. False when none runs to take it.
    fn resize(&mut self, pane: &str, size: Option<TermSize>) -> bool {
        self.term(pane)
            .map(|term| term.size.send_replace(size))
            .is_some()
    }

    /// Hands a running terminal view of `pane` a scroll. False when none
    /// runs to take it; a full queue drops the scroll.
    fn scroll(&self, pane: &str, lines: i32) -> bool {
        self.term(pane).is_some_and(|term| {
            !matches!(
                term.scrolls.try_send(lines),
                Err(mpsc::error::TrySendError::Closed(_))
            )
        })
    }
}

/// Follows what `inbound` asks for until it closes, or until `token` stops
/// signing `device` in, which ends with a `bye`.
pub(crate) async fn run(
    state: Shared,
    token: String,
    device: DeviceId,
    mut inbound: mpsc::Receiver<ClientMessage>,
    outbound: Outbound,
) {
    let mut views = Views::default();
    let lost = access_lost(&state, &token, &device);
    tokio::pin!(lost);
    loop {
        tokio::select! {
            message = inbound.recv() => match message {
                Some(message) => {
                    if let Some(error) = handle(&state, &mut views, message, &outbound) {
                        let message = ServerMessage::Error { message: error.to_owned() };
                        if outbound.send(message).await.is_err() {
                            return;
                        }
                    }
                }
                None => return,
            },
            reason = &mut lost => {
                drop(views);
                let _ = outbound.send(ServerMessage::Bye { reason }).await;
                return;
            }
        }
    }
}

/// Starts or stops a view; what the phone is told when it cannot.
fn handle(
    state: &Shared,
    views: &mut Views,
    message: ClientMessage,
    outbound: &Outbound,
) -> Option<&'static str> {
    let (view, since, size) = match message {
        ClientMessage::Unsubscribe { view } => {
            views.stop(&view);
            return None;
        }
        ClientMessage::Subscribe { view, since, size } => (view, since, size),
        ClientMessage::TermScroll { pane, lines } => {
            if lines == 0 {
                return Some("scroll out of range");
            }
            let lines = lines.clamp(-MAX_SCROLL_LINES, MAX_SCROLL_LINES);
            return (!views.scroll(&pane, lines)).then_some(CANNOT_SCROLL);
        }
    };
    if !views.admits(&view) {
        return Some("too many views on one connection");
    }
    let layout = state.hub.layout();
    let out = outbound.clone();
    let state = state.clone();
    match view.clone() {
        View::Home => views.start(view, follow_home(state, out)),
        View::Chat { pane } => {
            let Some(link) = layout.chat(&pane).and_then(|_| state.hub.chat(&pane)) else {
                return Some("no such chat");
            };
            views.start(view, follow_chat(state, link, since, out));
        }
        View::Term { pane } => {
            if layout.terminal(&pane).is_none() {
                return Some("no such terminal");
            }
            // A size out of range follows at the layout's size instead.
            let (size, refused) = match size {
                Some(size) if !size.is_valid() => (None, Some("terminal size out of range")),
                size => (size, None),
            };
            if !views.resize(&pane, size) {
                let (size, follow) = watch::channel(size);
                let (scrolls, scrolled) = mpsc::channel(SCROLL_QUEUE);
                views.start(
                    view,
                    follow_term(state, pane.clone(), follow, scrolled, out),
                );
                views.terms.insert(pane, TermControl { size, scrolls });
            }
            return refused;
        }
    }
    None
}

/// Returns once `token` no longer signs `device` in: revoked when the
/// device is gone, expired otherwise.
async fn access_lost(state: &Shared, token: &str, device: &DeviceId) -> ByeReason {
    let mut revocations = state.auth().watch_revocations();
    let mut check = tokio::time::interval(ACCESS_CHECK);
    loop {
        tokio::select! {
            _ = check.tick() => {}
            changed = revocations.changed() => if changed.is_err() {
                return ByeReason::Revoked;
            },
        }
        let mut auth = state.auth();
        if auth.authenticate(token, now_ms()).is_none() {
            return if auth.devices().get(device).is_some() {
                ByeReason::Expired
            } else {
                ByeReason::Revoked
            };
        }
    }
}

/// Sends home now and after every burst of changes in any chat, the
/// layout, or the pull requests; never twice the same, and never for
/// streamed text alone.
async fn follow_home(state: Shared, out: Outbound) {
    let mut all = state.hub.subscribe_all();
    let mut layout = state.hub.watch_layout();
    let mut pull_requests = state.pull_requests.subscribe();
    let mut load = state.load.clone();
    let mut sent = None;
    loop {
        let current = home(&state);
        if sent.as_ref() != Some(&current) {
            let message = ServerMessage::Home {
                home: current.clone(),
            };
            if out.send(message).await.is_err() {
                return;
            }
            sent = Some(current);
        }
        loop {
            tokio::select! {
                event = all.recv() => match event {
                    Ok((_, Event::Change { change, .. })) if matches!(*change, Change::Append { .. }) => continue,
                    Ok(_) | Err(RecvError::Lagged(_)) => break,
                    Err(RecvError::Closed) => return,
                },
                changed = layout.changed() => if changed.is_err() { return } else { break },
                changed = pull_requests.changed() => if changed.is_err() { return } else { break },
                // A stopped sampler leaves the last figures standing.
                changed = load.changed(), if load.has_changed().is_ok() => if changed.is_ok() { break },
                () = out.closed() => return,
            }
        }
        tokio::time::sleep(HOME_COALESCE).await;
        while all.try_recv().is_ok() {}
    }
}

/// What gathered of a chat's stream since the last send.
#[derive(Default)]
struct Batch {
    reset: Option<(Arc<Transcript>, Cursor)>,
    steps: Vec<Step>,
    connection: bool,
}

impl Batch {
    /// Takes `event` unless the phone has it already: changes up to `held`
    /// came with the snapshot or a send before.
    fn take(&mut self, event: Event, held: &mut Option<Cursor>) {
        match event {
            Event::Reset { transcript, cursor } => {
                self.steps.clear();
                self.reset = Some((transcript, cursor));
                *held = Some(cursor);
            }
            Event::Change { change, cursor } => {
                let had =
                    held.is_some_and(|held| held.epoch == cursor.epoch && cursor.seq <= held.seq);
                if !had {
                    self.steps.push(Step {
                        cursor,
                        change: (*change).clone(),
                    });
                    *held = Some(cursor);
                }
            }
            Event::Connection => self.connection = true,
        }
    }

    /// The messages to send, in order: a new snapshot, the changes after
    /// it, then the connection's state.
    fn messages(self, pane: &str, connected: bool) -> Vec<ServerMessage> {
        let mut messages = Vec::new();
        if let Some((transcript, cursor)) = self.reset {
            messages.push(ServerMessage::ChatSnapshot(Box::new(ChatSnapshot {
                pane: pane.to_owned(),
                cursor: Some(cursor),
                connected,
                transcript: Transcript::clone(&transcript),
            })));
        }
        messages.extend(split_at_pictures(self.steps).into_iter().map(|changes| {
            ServerMessage::ChatChanges {
                pane: pane.to_owned(),
                changes,
            }
        }));
        if self.connection {
            messages.push(ServerMessage::ChatConnection {
                pane: pane.to_owned(),
                connected,
            });
        }
        messages
    }
}

/// Gathered changes as the `chat_changes` to send, a new one starting at
/// each picture. A picture is base64 of up to a few megabytes, so on a
/// slow link the changes before it, such as the prompt it came with, show
/// at once rather than after it arrives.
pub(super) fn split_at_pictures(steps: Vec<Step>) -> Vec<Vec<Step>> {
    let mut batches: Vec<Vec<Step>> = Vec::new();
    for step in steps {
        let picture = matches!(
            &step.change,
            Change::Push { entry } if matches!(entry.kind, EntryKind::Image { .. })
        );
        match batches.last_mut() {
            Some(batch) if !picture => batch.push(step),
            _ => batches.push(vec![step]),
        }
    }
    batches
}

/// The first answer to a chat subscription: nothing new when the phone's
/// cursor is the chat's, else the whole transcript.
pub(crate) fn opening(
    pane: &str,
    since: Option<Cursor>,
    base: (Transcript, Option<Cursor>),
    connected: bool,
) -> ServerMessage {
    let (transcript, cursor) = base;
    match cursor {
        Some(cursor) if since == Some(cursor) => ServerMessage::ChatResumed {
            pane: pane.to_owned(),
            cursor,
            connected,
        },
        _ => ServerMessage::ChatSnapshot(Box::new(ChatSnapshot {
            pane: pane.to_owned(),
            cursor,
            connected,
            transcript,
        })),
    }
}

async fn send_all(out: &Outbound, messages: Vec<ServerMessage>) -> bool {
    for message in messages {
        if out.send(message).await.is_err() {
            return false;
        }
    }
    true
}

/// Sends the chat's opening, then its changes in bursts of
/// [`CHAT_COALESCE`], until the phone or the chat goes away. A phone that
/// fell behind the chat's queue is told to reload.
async fn follow_chat(state: Shared, link: Arc<Chat>, since: Option<Cursor>, out: Outbound) {
    let pane = link.pane.clone();
    let reload = ServerMessage::Reload {
        view: View::Chat { pane: pane.clone() },
    };
    // Subscribing first, then reading the base, misses nothing: changes
    // queued meanwhile are skipped by their cursor.
    let (_, mut events) = link.subscribe();
    let base = link.base();
    let mut held = base.1;
    if out
        .send(opening(&pane, since, base, link.summary().connected))
        .await
        .is_err()
    {
        return;
    }
    let mut layout = state.hub.watch_layout();
    loop {
        let mut batch = Batch::default();
        tokio::select! {
            event = events.recv() => match event {
                Ok(event) => batch.take(event, &mut held),
                Err(_) => {
                    let _ = out.send(reload).await;
                    return;
                }
            },
            changed = layout.changed() => {
                if changed.is_err() {
                    return;
                }
                if state.hub.layout().chat(&pane).is_none() {
                    let _ = out.send(reload).await;
                    return;
                }
                continue;
            }
            () = out.closed() => return,
        }
        let gather = tokio::time::sleep(CHAT_COALESCE);
        tokio::pin!(gather);
        loop {
            tokio::select! {
                () = &mut gather => break,
                event = events.recv() => match event {
                    Ok(event) => batch.take(event, &mut held),
                    Err(_) => {
                        let _ = out.send(reload).await;
                        return;
                    }
                },
            }
        }
        if !send_all(&out, batch.messages(&pane, link.summary().connected)).await {
            return;
        }
    }
}

/// Sends the pane's screen whenever it changes, read on a timer and again
/// at once after typing ([`Typed`](crate::web::Typed)): Herdr's API has no
/// output events. While `size` names one, Herdr holds the terminal at it,
/// and `scrolls` scroll it.
async fn follow_term(
    state: Shared,
    pane: String,
    mut size: watch::Receiver<Option<TermSize>>,
    mut scrolls: mpsc::Receiver<i32>,
    out: Outbound,
) {
    let mut pace = state.typed.pace(&pane);
    let reload = ServerMessage::Reload {
        view: View::Term { pane: pane.clone() },
    };
    let mut last = None;
    let mut failures = 0;
    // Dropped on every exit, which lets Herdr size the terminal back.
    let mut sizing = term_size::Sizing::default();
    // The phone hears once per view that this terminal cannot scroll.
    let mut told = false;
    loop {
        if state.hub.layout().terminal(&pane).is_none() {
            let _ = out.send(reload).await;
            return;
        }
        let wanted = *size.borrow_and_update();
        sizing.apply(&state, &pane, wanted).await;
        match read_screen(&state, &pane).await {
            Ok(text) => {
                failures = 0;
                if last.as_ref() != Some(&text) {
                    let (columns, rows) = model::screen(&text);
                    let message = ServerMessage::TermScreen {
                        pane: pane.clone(),
                        columns,
                        rows,
                    };
                    if out.send(message).await.is_err() {
                        return;
                    }
                    last = Some(text);
                }
            }
            Err(_) => {
                failures += 1;
                if failures >= SCREEN_FAILURES {
                    let _ = out.send(reload).await;
                    return;
                }
            }
        }
        tokio::select! {
            () = pace.wait() => {}
            () = out.closed() => return,
            // Subscribed again: the screen goes out again, at the new size.
            changed = size.changed() => {
                if changed.is_err() {
                    return;
                }
                last = None;
            }
            Some(lines) = scrolls.recv() => {
                if sizing.scroll(lines) {
                    // Read at once and again shortly, as after typing.
                    state.typed.typed(&pane);
                } else if !told {
                    told = true;
                    let message = ServerMessage::Error { message: CANNOT_SCROLL.to_owned() };
                    if out.send(message).await.is_err() {
                        return;
                    }
                }
            }
        }
    }
}
