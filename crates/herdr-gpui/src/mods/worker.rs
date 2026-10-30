//! One mod's thread. It owns the mod's Luau state, reloads it when its
//! files change, keeps its timers, and runs `view` when a watched topic
//! changed or `update` ran, at most every [`VIEW_GAP`]. Both directions go
//! through bounded mailboxes: the window's latest state and a short queue of
//! messages in; the latest tree and error, log lines, and commands out.

use herdr_mods::{
    Data, Effect, Error, Limits, LogLine, Manifest, Mod, Node, Pace, Sources, Stamp, State,
};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError},
    thread,
    time::{Duration, Instant, SystemTime},
};

/// The fastest a mod redraws, whatever wakes it.
const VIEW_GAP: Duration = Duration::from_millis(100);
/// How often a mod's files are checked for changes.
const SCAN: Duration = Duration::from_secs(1);
const MAX_MESSAGES: usize = 32;
const MAX_TIMERS: usize = 32;
const MAX_LOG: usize = 200;
const MAX_COMMANDS: usize = 8;

#[derive(Default)]
struct Inbox {
    state: Option<Arc<State>>,
    messages: VecDeque<Data>,
    reload: bool,
    stopped: bool,
}

/// What the worker reported since the window last took it.
#[derive(Default)]
pub(super) struct Report {
    pub manifest: Option<Manifest>,
    /// The activity bar icon's SVG, sent with each manifest.
    pub icon: Option<Option<String>>,
    /// The latest tree; `Some(None)` when the view drew nothing.
    pub tree: Option<Option<Node>>,
    /// Why the last `update` or `view` failed; `Some(None)` clears it once
    /// the mod draws again.
    pub error: Option<Option<Error>>,
    /// Why the mod's files did not load; `Some(None)` clears it once a
    /// reload succeeds.
    pub load_error: Option<Option<Error>>,
    pub log: Vec<LogLine>,
    pub commands: Vec<herdr_commands::Command>,
}

impl Report {
    pub(super) fn changed(&self) -> bool {
        self.manifest.is_some()
            || self.tree.is_some()
            || self.error.is_some()
            || self.load_error.is_some()
    }
}

struct Link {
    inbox: Mutex<Inbox>,
    wake: Condvar,
    outbox: Mutex<Report>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The window's end of a worker. Dropping it stops the thread after its
/// current call; nothing waits for it.
pub(super) struct Handle {
    name: String,
    link: Arc<Link>,
}

impl Handle {
    pub(super) fn spawn(dir: PathBuf) -> Result<Self, super::Error> {
        let name = dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let link = Arc::new(Link {
            inbox: Mutex::new(Inbox {
                reload: true,
                ..Inbox::default()
            }),
            wake: Condvar::new(),
            outbox: Mutex::new(Report::default()),
        });
        let worker = Arc::clone(&link);
        thread::Builder::new()
            .name(format!("herdr-mod-{name}"))
            .spawn(move || Worker::new(dir, worker).run())
            .map_err(|source| super::Error::Spawn {
                name: name.clone(),
                source,
            })?;
        Ok(Self { name, link })
    }

    pub(super) fn offer(&self, state: Arc<State>) {
        lock(&self.link.inbox).state = Some(state);
        self.link.wake.notify_one();
    }

    /// Queues a message from a click or the palette.
    pub(super) fn send(&self, msg: Data) -> Result<(), super::Error> {
        let mut inbox = lock(&self.link.inbox);
        if inbox.messages.len() == MAX_MESSAGES {
            return Err(super::Error::Busy(self.name.clone()));
        }
        inbox.messages.push_back(msg);
        drop(inbox);
        self.link.wake.notify_one();
        Ok(())
    }

    pub(super) fn reload(&self) {
        lock(&self.link.inbox).reload = true;
        self.link.wake.notify_one();
    }

    pub(super) fn take(&self) -> Report {
        std::mem::take(&mut *lock(&self.link.outbox))
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        lock(&self.link.inbox).stopped = true;
        self.link.wake.notify_one();
    }
}

/// A loaded mod's sources, state, and first view, before it may replace
/// the running one.
type Candidate = (Sources, Mod, Result<Option<Node>, Error>);

struct Worker {
    dir: PathBuf,
    link: Arc<Link>,
    module: Option<Mod>,
    stamp: Option<Stamp>,
    next_scan: Instant,
    timers: Vec<(Instant, Data)>,
    state: Arc<State>,
    pace: Pace,
    dirty: bool,
}

impl Worker {
    fn new(dir: PathBuf, link: Arc<Link>) -> Self {
        Self {
            dir,
            link,
            module: None,
            stamp: None,
            next_scan: Instant::now(),
            timers: Vec::new(),
            state: Arc::new(State::default()),
            pace: Pace::new(VIEW_GAP),
            dirty: false,
        }
    }

    fn run(mut self) {
        loop {
            let now = Instant::now();
            let mut due = self.next_scan;
            if let Some((at, _)) = self.timers.iter().min_by_key(|(at, _)| *at) {
                due = due.min(*at);
            }
            if self.dirty && self.module.is_some() {
                due = due.min(now + self.pace.wait(now));
            }
            let Some(inbox) = self.wait(due) else {
                return;
            };
            let now = Instant::now();
            // State first, so a load in this pass draws against it.
            if let Some(state) = inbox.state {
                let watched = self.module.as_ref().is_some_and(|module| {
                    module
                        .topics()
                        .iter()
                        .any(|topic| self.state.changed(&state, *topic))
                });
                self.dirty |= watched;
                self.state = state;
            }
            if inbox.reload || now >= self.next_scan {
                self.next_scan = now + SCAN;
                self.check_files(inbox.reload);
            }
            for msg in inbox.messages {
                self.update(&msg, true);
            }
            let fired: Vec<Data> = {
                let (due, waiting) = std::mem::take(&mut self.timers)
                    .into_iter()
                    .partition::<Vec<_>, _>(|(at, _)| *at <= now);
                self.timers = waiting;
                due.into_iter().map(|(_, msg)| msg).collect()
            };
            for msg in fired {
                self.update(&msg, false);
            }
            if self.dirty && self.module.is_some() && self.pace.take(now) {
                self.dirty = false;
                self.view();
            }
        }
    }

    /// Waits for mail or `due`; `None` once the window let go.
    fn wait(&self, due: Instant) -> Option<Inbox> {
        let mut inbox = lock(&self.link.inbox);
        loop {
            if inbox.stopped {
                return None;
            }
            let now = Instant::now();
            if inbox.state.is_some() || !inbox.messages.is_empty() || inbox.reload || now >= due {
                return Some(std::mem::take(&mut *inbox));
            }
            inbox = self
                .link
                .wake
                .wait_timeout(inbox, due - now)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    /// Loads the mod again when its files changed. A candidate replaces a
    /// running mod only once it loaded and drew its first view against the
    /// current state; until then the last good mod keeps running, and why
    /// the new files failed stays reported until a reload succeeds. With no
    /// mod running yet, a candidate that loaded is kept even when its first
    /// view fails, so it draws again once a topic it watches changes.
    fn check_files(&mut self, force: bool) {
        let stamp = match Stamp::take(&self.dir) {
            Ok(stamp) => stamp,
            Err(error) => return self.fail_load(error),
        };
        if !force && self.stamp.as_ref() == Some(&stamp) {
            return;
        }
        self.stamp = Some(stamp);
        let mut log = Vec::new();
        let candidate = self.candidate(&mut log);
        push_log(&mut lock(&self.link.outbox), log);
        let (sources, module, tree) = match candidate {
            Ok((sources, module, Ok(tree))) => (sources, module, Some(tree)),
            Ok((sources, module, Err(error))) if self.module.is_none() => {
                self.fail(error);
                (sources, module, None)
            }
            Ok((_, _, Err(error))) | Err(error) => return self.fail_load(error),
        };
        self.module = Some(module);
        self.timers.clear();
        self.dirty = false;
        let icon = sources
            .manifest
            .activity
            .as_ref()
            .and_then(|activity| sources.icon(&activity.icon))
            .map(str::to_owned);
        let mut outbox = lock(&self.link.outbox);
        outbox.icon = Some(icon);
        outbox.manifest = Some(sources.manifest);
        outbox.load_error = Some(None);
        if let Some(tree) = tree {
            outbox.tree = Some(tree);
            outbox.error = Some(None);
        }
    }

    /// Reads and loads the mod's files and draws them once, leaving what
    /// it logged in `log`. Loading must work; the first view's result is
    /// the caller's to judge.
    fn candidate(&self, log: &mut Vec<LogLine>) -> Result<Candidate, Error> {
        let sources = Sources::read(&self.dir)?;
        let now = SystemTime::now();
        let mut module = Mod::load(&sources, limits(), now)?;
        let tree = module.view(&self.state, now);
        log.extend(module.take_log());
        Ok((sources, module, tree))
    }

    fn update(&mut self, msg: &Data, from_user: bool) {
        let Some(module) = &mut self.module else {
            return;
        };
        let result = module.update(msg, SystemTime::now());
        let mut log = module.take_log();
        match result {
            Ok(effects) => {
                self.dirty = true;
                let mut commands = Vec::new();
                for effect in effects {
                    match effect {
                        // Only a person's click or palette pick runs app
                        // commands; a timer could otherwise close panes alone.
                        Effect::Run(command) if from_user => commands.push(command),
                        Effect::Run(_) => log.push(LogLine {
                            at: SystemTime::now(),
                            text: "herdr.run ignored: only a click or a palette command may run app commands".to_owned(),
                        }),
                        Effect::Timer { after, msg } if self.timers.len() < MAX_TIMERS => {
                            self.timers.push((Instant::now() + after, msg));
                        }
                        Effect::Timer { .. } => log.push(LogLine {
                            at: SystemTime::now(),
                            text: format!("herdr.timer ignored: more than {MAX_TIMERS} pending"),
                        }),
                    }
                }
                let mut outbox = lock(&self.link.outbox);
                for command in commands {
                    if outbox.commands.len() < MAX_COMMANDS {
                        outbox.commands.push(command);
                    }
                }
                push_log(&mut outbox, log);
            }
            Err(error) => {
                push_log(&mut lock(&self.link.outbox), log);
                self.fail(error);
            }
        }
    }

    fn view(&mut self) {
        let Some(module) = &mut self.module else {
            return;
        };
        let result = module.view(&self.state, SystemTime::now());
        let log = module.take_log();
        let mut outbox = lock(&self.link.outbox);
        push_log(&mut outbox, log);
        match result {
            Ok(tree) => {
                outbox.tree = Some(tree);
                outbox.error = Some(None);
            }
            Err(error) => {
                drop(outbox);
                self.fail(error);
            }
        }
    }

    /// A failed `update` or `view`; the next good view clears it.
    fn fail(&self, error: Error) {
        lock(&self.link.outbox).error = Some(Some(error));
    }

    /// Files that could not be read, loaded, or drawn; only a successful
    /// reload clears it, so a broken save stays reported while the last
    /// good mod runs on.
    fn fail_load(&self, error: Error) {
        lock(&self.link.outbox).load_error = Some(Some(error));
    }
}

fn push_log(outbox: &mut Report, lines: Vec<LogLine>) {
    outbox.log.extend(lines);
    let excess = outbox.log.len().saturating_sub(MAX_LOG);
    outbox.log.drain(..excess);
}

fn limits() -> Limits {
    Limits::default()
}
