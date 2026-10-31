//! One thread per shown terminal. It owns the attached `Terminal`, whatever
//! backend made it, applies the view's commands between waits for output,
//! and keeps the latest screen in a mailbox the window reads on its tick.
//! Nothing here runs on the UI thread, and the UI never waits for it.

use super::screen::Screen;
use herdr_backend::{
    Backend, ClientPaneInputEvent, Role, Size, SurfaceUpdate, Terminal, TerminalId,
};
use std::{
    sync::{
        Arc, Mutex, PoisonError,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

/// How long a wait for output lasts before queued commands are applied.
const POLL: Duration = Duration::from_millis(8);
/// Commands queued for one terminal; typing faster than this drops keys
/// with a warning rather than growing without bound.
const MAX_COMMANDS: usize = 256;

/// What the view asks of its terminal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Focus(bool),
    Input(ClientPaneInputEvent),
    Resize(Size),
}

/// The latest screen, and whether the window has seen it.
#[derive(Default)]
pub(crate) struct Mailbox {
    changed: AtomicBool,
    screen: Mutex<Screen>,
    stop: AtomicBool,
}

impl Mailbox {
    fn update(&self, change: impl FnOnce(&mut Screen)) {
        change(&mut self.screen.lock().unwrap_or_else(PoisonError::into_inner));
        self.changed.store(true, Ordering::Release);
    }

    pub(crate) fn changed(&self) -> bool {
        self.changed.load(Ordering::Acquire)
    }

    /// The screen, when it changed since the last take.
    pub(crate) fn take(&self) -> Option<Screen> {
        self.changed.swap(false, Ordering::AcqRel).then(|| {
            self.screen
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        })
    }
}

/// The view's end of a terminal's worker. Dropping it stops the worker on
/// its next wait; nothing joins it.
pub(crate) struct Link {
    commands: std::sync::mpsc::SyncSender<Command>,
    pub(crate) mailbox: Arc<Mailbox>,
}

impl Link {
    pub(crate) fn send(&self, command: Command) {
        if let Err(error) = self.commands.try_send(command) {
            tracing::warn!(?error, "Dropped input for a herdr-core terminal");
        }
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.mailbox.stop.store(true, Ordering::Release);
    }
}

/// Attaches `id` on a new thread and keeps its screen in the returned link.
pub(crate) fn spawn<B>(backend: B, id: TerminalId, size: Size, role: Role) -> std::io::Result<Link>
where
    B: Backend + 'static,
{
    let (commands, queue) = std::sync::mpsc::sync_channel(MAX_COMMANDS);
    let mailbox = Arc::new(Mailbox::default());
    let shared = Arc::clone(&mailbox);
    std::thread::Builder::new()
        .name("herdr-core-terminal".into())
        .spawn(move || run(&backend, &id, size, role, &queue, &shared))?;
    Ok(Link { commands, mailbox })
}

fn run<B: Backend>(
    backend: &B,
    id: &TerminalId,
    size: Size,
    role: Role,
    queue: &std::sync::mpsc::Receiver<Command>,
    mailbox: &Mailbox,
) {
    let fail = |error: &dyn std::error::Error| {
        tracing::info!(%error, terminal = %id, "herdr-core terminal stopped");
        mailbox.update(|screen| screen.failure = Some(error.to_string()));
    };
    let mut terminal = match backend.attach(id, size, role) {
        Ok(terminal) => terminal,
        Err(error) => return fail(&error),
    };
    while !mailbox.stop.load(Ordering::Acquire) {
        for command in queue.try_iter() {
            let applied = match command {
                Command::Focus(focused) => terminal.focus(focused),
                Command::Input(input) => terminal.input(&input),
                Command::Resize(size) => terminal.resize(size),
            };
            if let Err(error) = applied {
                return fail(&error);
            }
        }
        match terminal.next(POLL) {
            Ok(updates) if updates.is_empty() => {}
            Ok(updates) => {
                let ended = updates
                    .iter()
                    .any(|update| matches!(update, SurfaceUpdate::Ended(_)));
                mailbox
                    .update(|screen| updates.into_iter().for_each(|update| screen.apply(update)));
                // An ended terminal's last screen stays shown; its stream
                // answers at once from here on, so waiting again would spin.
                if ended {
                    return;
                }
            }
            Err(error) => return fail(&error),
        }
    }
}
