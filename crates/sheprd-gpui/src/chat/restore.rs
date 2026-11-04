//! Continuing chats in the panes Herdr restores after a reboot or a server
//! restart. Herdr brings a pane back as a shell in its folder, without the
//! `herdr-acp` that ran there or the token it announced; the program left a
//! record of how to continue it (`sheprd_chat_protocol::restore`), and this
//! window types `herdr-acp --resume` into the restored shell, once. The new
//! program announces itself, and discovery attaches its view as for any chat.
//!
//! Typing into a pane is only safe into the right idle shell, so a record is
//! acted on only when all of these hold, checked off the UI thread:
//!
//! - its chat was seen live under an earlier boot of this daemon, never the
//!   current one: a chat that died while its daemon kept running stays dead;
//! - it names this daemon (its Herdr session) and a pane this snapshot has,
//!   with no agent or chat reported in it, and no other record names it;
//! - nothing listens on its endpoint any more;
//! - Herdr says the pane's shell itself is in the foreground, in the
//!   record's folder;
//! - this window claimed the record first, so of several windows or apps
//!   only one continues the chat;
//! - the window still shows that boot and connection, and the pane is still
//!   free, when the line is typed.
//!
//! A typed line is only queued, so the record stays claimed until a chat
//! shows in its pane, and is given back if the boot or connection changes
//! first, chats are turned off, or none shows in time. Claims a dropped
//! window or a failed job held are given back too, and `prune` returns any
//! an exited app left behind; a record is removed only once its chat shows.
//!
//! While a window is connected it also watches the chats it saw live: one
//! missing from the snapshot for [`plan::ENDED_AFTER`] under the same boot
//! ended there (its pane closed, or the program was killed), and its record
//! goes, so a later pane that reuses its id is never handed the old chat.
//! A chat newly live in a pane also replaces any older record naming it.
//!
//! The pass runs [`plan::PASSES`] times, [`plan::PASS_INTERVAL`] apart, for
//! each daemon boot and connection, since a restored shell may still be
//! starting when the first snapshot arrives. It runs only for a daemon on
//! this machine whose `herdr` CLI can be asked about a pane's processes
//! (Linux and macOS).

pub(super) mod plan;

use plan::{
    PASSES, Pane, Probes, Scope, Typed, Watched, every_record, free, idle, live_chats, panes, pass,
    pass_due, settle, superseded, to_stamp,
};

use super::{Route, discovery};
use crate::{SheprdWindow, listening_ports, processes};
use sheprd_chat_protocol::{EndpointId, restore};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, mpsc},
    time::{Duration, Instant},
};

/// How often live chats are stamped with the boot they run under, and
/// watched for having ended.
const STAMP_INTERVAL: Duration = Duration::from_secs(5);
/// How often the window looks at all, between jobs.
const CHECK_INTERVAL: Duration = Duration::from_secs(1);
/// What the window learned off the UI thread.
pub(super) enum Outcome {
    /// Records a pass claimed, each with the line that continues it; when
    /// the pass finished; and where the next pass reads on from.
    Claimed {
        at: Instant,
        claimed: Vec<(restore::Record, String)>,
        next: Option<EndpointId>,
        scope: Scope,
    },
    /// Live chats whose records now carry the current boot.
    Stamped(Scope, Vec<EndpointId>),
}

impl Outcome {
    /// Gives back what a pass claimed, as one nobody will act on. Blocking.
    fn give_back(self, dir: &Path) {
        if let Self::Claimed { claimed, .. } = self {
            for (record, _) in claimed {
                let _ = restore::unclaim(dir, &record.endpoint);
            }
        }
    }
}

/// The window's restore state. One background job runs at a time.
#[derive(Default)]
pub(crate) struct Restore {
    /// Where the next pass reads on from.
    cursor: Option<EndpointId>,
    job: Option<mpsc::Receiver<Outcome>>,
    next_check: Option<Instant>,
    /// When the restore pass may run again for [`Self::passes`]' scope.
    next_pass: Option<Instant>,
    next_stamp: Option<Instant>,
    /// The scope the restore pass ran for, and how many times.
    passes: Option<(Scope, u8)>,
    stamped: HashSet<EndpointId>,
    stamped_scope: Option<Scope>,
    /// Lines typed whose chat has not shown yet; their records stay claimed.
    typed: Vec<Typed>,
    watched: Watched,
}

/// A window going away gives back every claim it holds: those of a pass
/// whose result it will never read, and those of lines whose chat never
/// showed. A pass still running gives its own back when it cannot report.
impl Drop for Restore {
    fn drop(&mut self) {
        let outcome = self.job.take().and_then(|job| job.try_recv().ok());
        let typed = std::mem::take(&mut self.typed);
        if outcome.is_none() && typed.is_empty() {
            return;
        }
        let Some(dir) = restore::restore_dir() else {
            return;
        };
        detach(move || {
            if let Some(outcome) = outcome {
                outcome.give_back(&dir);
            }
            for line in typed {
                let _ = restore::unclaim(&dir, &line.endpoint);
            }
        });
    }
}

/// Whether a program still listens on `endpoint`. Only a refused or missing
/// socket counts as gone; anything else is taken as alive.
fn listening(endpoint: &EndpointId) -> bool {
    #[cfg(unix)]
    {
        let Some(address) = sheprd_chat_protocol::endpoint_address(endpoint) else {
            return false;
        };
        match std::os::unix::net::UnixStream::connect(address) {
            Ok(_) => true,
            Err(error) => !matches!(
                error.kind(),
                std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound
            ),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = endpoint;
        true
    }
}

/// Whether the pane's shell is at its prompt in `cwd`, as the daemon's CLI
/// reports it. Blocking.
fn idle_in(daemon: &processes::Daemon, pane: &Pane, cwd: &Path) -> bool {
    let cancelled = AtomicBool::new(false);
    daemon.process_info(&pane.id, &cancelled).is_ok_and(|info| {
        idle(&info, pane.cwd.as_deref(), cwd, |path| {
            std::fs::canonicalize(path).ok()
        })
    })
}

/// Whether a record's Herdr API socket is `owner`'s.
fn owned_by(owner: &listening_ports::Daemon, socket: &Path) -> bool {
    socket.to_str().is_some_and(|socket| owner.owns(socket))
}

/// Runs `job` on a thread of its own; the window reads its outcome on a
/// later tick. When the window has stopped listening, the job gives back
/// what it claimed. A thread that cannot start leaves nothing to read.
pub(super) fn spawn(
    dir: PathBuf,
    job: impl FnOnce(&Path) -> Outcome + Send + 'static,
) -> Option<mpsc::Receiver<Outcome>> {
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("chat-restore".to_owned())
        .spawn(move || {
            if let Err(mpsc::SendError(outcome)) = sender.send(job(&dir)) {
                outcome.give_back(&dir);
            }
        })
        .ok()?;
    Some(receiver)
}

/// Runs `work` on a thread of its own and forgets it: removing or giving
/// back a record needs no answer.
fn detach(work: impl FnOnce() + Send + 'static) {
    let _ = std::thread::Builder::new()
        .name("chat-restore".to_owned())
        .spawn(work);
}

/// What the restore pass and the watch need of the selected daemon.
struct Target {
    daemon: processes::Daemon,
    owner: listening_ports::Daemon,
    scope: Scope,
}

impl SheprdWindow {
    /// Runs every tick: runs the restore pass for the daemon's boot,
    /// continues the chats it claimed, settles the lines it typed, and in
    /// between stamps and watches live chats. With chats off it only gives
    /// back what it holds.
    pub(super) fn poll_chat_restore(&mut self, enabled: bool, route: Option<&Route>) {
        let restore = &self.chats.restore;
        if !enabled && restore.job.is_none() && restore.typed.is_empty() {
            return;
        }
        let Some(dir) = restore::restore_dir() else {
            return;
        };
        let now = Instant::now();
        self.finish_chat_restore(&dir, enabled, now);
        let target = enabled.then(|| self.chat_restore_target(route)).flatten();
        self.settle_typed_chats(&dir, target.as_ref().map(|target| &target.scope), now);
        let restore = &mut self.chats.restore;
        let Some(Target {
            daemon,
            owner,
            scope,
        }) = target
        else {
            return;
        };
        if restore.job.is_some() || restore.next_check.is_some_and(|next| now < next) {
            return;
        }
        restore.next_check = Some(now + CHECK_INTERVAL);
        let Some(snapshot) = self.live.snapshot.as_deref() else {
            return;
        };
        if let Some(run) = pass_due(restore.passes.as_ref(), restore.next_pass, &scope, now)
            && let Ok(program) = self.chats.program.found().map(Path::to_owned)
        {
            let panes = panes(snapshot);
            let cursor = restore.cursor.clone();
            restore.passes = Some((scope.clone(), run));
            restore.next_pass = Some(now + plan::PASS_INTERVAL);
            let last = run == PASSES;
            restore.job = spawn(dir, move |dir| {
                let probes = Probes {
                    owns: |socket: &Path| owned_by(&owner, socket),
                    listening,
                    idle: |pane: &Pane, cwd: &Path| idle_in(&daemon, pane, cwd),
                };
                let passed = pass(
                    dir,
                    cursor.as_ref(),
                    &panes,
                    &scope.boot,
                    &program,
                    last,
                    probes,
                );
                Outcome::Claimed {
                    at: Instant::now(),
                    claimed: passed.claimed,
                    next: passed.next,
                    scope,
                }
            });
            return;
        }
        let live = live_chats(snapshot);
        self.watch_live_chats(dir, scope, owner, live, now);
    }

    /// The selected daemon, when its chats can be continued: on this
    /// machine, named, asked about through its CLI, and connected.
    fn chat_restore_target(&self, route: Option<&Route>) -> Option<Target> {
        if route != Some(&Route::Local) {
            return None;
        }
        let endpoint = self.endpoints.get(self.selected_endpoint)?;
        let target = &endpoint.connection.target;
        // A daemon whose socket cannot be named could own any record.
        let owner =
            listening_ports::Daemon::of(target).filter(listening_ports::Daemon::identified)?;
        let daemon = processes::Daemon::for_target(target)?;
        let snapshot = self
            .live
            .snapshot
            .as_deref()
            .filter(|_| self.live.status.is_connected())?;
        Some(Target {
            daemon,
            owner,
            scope: Scope {
                boot: snapshot.boot_id.clone(),
                connection: endpoint.generation,
                selection: self.selection_epoch,
            },
        })
    }

    /// Settles the typed lines against `current`, the scope the window
    /// shows (none while it cannot continue chats): removes the records of
    /// chats that showed, and gives back the rest that cannot.
    fn settle_typed_chats(&mut self, dir: &Path, current: Option<&Scope>, now: Instant) {
        if self.chats.restore.typed.is_empty() {
            return;
        }
        let chat_panes: HashSet<&str> = self
            .live
            .snapshot
            .as_deref()
            .map(|snapshot| {
                snapshot
                    .agents
                    .iter()
                    .filter(|agent| discovery::endpoint(&agent.tokens).is_some())
                    .map(|agent| agent.pane_id.as_str())
                    .collect()
            })
            .unwrap_or_default();
        let settled = settle(&mut self.chats.restore.typed, current, &chat_panes, now);
        if settled.confirmed.is_empty() && settled.returned.is_empty() {
            return;
        }
        let dir = dir.to_owned();
        detach(move || {
            for endpoint in &settled.confirmed {
                restore::remove(&dir, endpoint);
            }
            for endpoint in &settled.returned {
                let _ = restore::unclaim(&dir, endpoint);
            }
        });
    }

    /// Now and then: drops the records of chats that ended under the
    /// running daemon, judged against every chat `live` in the snapshot,
    /// and of those a chat newly live in their pane replaced, and stamps the
    /// records of the live chats with the scope's boot, each once per scope
    /// and a bounded batch at a time.
    fn watch_live_chats(
        &mut self,
        dir: PathBuf,
        scope: Scope,
        owner: listening_ports::Daemon,
        live: Vec<(EndpointId, String)>,
        now: Instant,
    ) {
        let restore = &mut self.chats.restore;
        if restore.next_stamp.is_some_and(|next| now < next) {
            return;
        }
        restore.next_stamp = Some(now + STAMP_INTERVAL);
        let endpoints: Vec<EndpointId> =
            live.iter().map(|(endpoint, _)| endpoint.clone()).collect();
        let ended = restore.watched.observe(&scope, &endpoints, now);
        if restore.stamped_scope.as_ref() != Some(&scope) {
            restore.stamped.clear();
            restore.stamped_scope = Some(scope.clone());
        }
        let unstamped = to_stamp(live, &restore.stamped);
        if unstamped.is_empty() && ended.is_empty() {
            return;
        }
        restore.job = spawn(dir, move |dir| {
            for endpoint in &ended {
                restore::remove(dir, endpoint);
            }
            if !unstamped.is_empty() {
                let records = every_record(dir);
                for endpoint in superseded(&records, &unstamped, |socket| owned_by(&owner, socket))
                {
                    restore::remove(dir, endpoint);
                }
            }
            let stamped = unstamped
                .into_iter()
                .map(|(endpoint, _)| endpoint)
                .filter(|endpoint| {
                    restore::write_seen(dir, endpoint, &scope.boot).is_ok()
                        && restore::read_seen(dir, endpoint).as_deref() == Some(&scope.boot)
                })
                .collect();
            Outcome::Stamped(scope, stamped)
        });
    }

    /// Applies a finished job: remembers stamped chats, and types the line
    /// into each claimed pane that is still as the pass found it, while
    /// chats are on and the claim is fresh, holding its record claimed until
    /// the chat shows; any other claim is given back.
    fn finish_chat_restore(&mut self, dir: &Path, enabled: bool, now: Instant) {
        let Some(job) = &self.chats.restore.job else {
            return;
        };
        let outcome = match job.try_recv() {
            Ok(outcome) => outcome,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.chats.restore.job = None;
                return;
            }
        };
        self.chats.restore.job = None;
        match outcome {
            Outcome::Stamped(scope, stamped) => {
                if self.chats.restore.stamped_scope.as_ref() == Some(&scope) {
                    self.chats.restore.stamped.extend(stamped);
                }
            }
            Outcome::Claimed {
                at,
                claimed,
                next,
                scope,
            } => {
                self.chats.restore.cursor = next;
                let fresh = enabled && now.saturating_duration_since(at) <= plan::CLAIM_FRESH;
                let mut returned = Vec::new();
                for (record, line) in claimed {
                    if fresh && self.continue_restored_chat(&scope, &record, &line) {
                        tracing::info!(
                            pane = %record.pane,
                            agent = %record.agent,
                            "Continuing a chat in its restored pane"
                        );
                        self.chats.restore.typed.push(Typed {
                            endpoint: record.endpoint,
                            pane: record.pane,
                            scope: scope.clone(),
                            since: now,
                        });
                    } else {
                        tracing::debug!(
                            pane = %record.pane,
                            "A restored chat's pane or connection changed; not continuing it"
                        );
                        returned.push(record.endpoint);
                    }
                }
                if !returned.is_empty() {
                    let dir = dir.to_owned();
                    detach(move || {
                        for endpoint in &returned {
                            let _ = restore::unclaim(&dir, endpoint);
                        }
                    });
                }
            }
        }
    }

    /// Types `line` into the record's pane when the window still shows the
    /// boot and connection the pass ran for and the pane is still free.
    fn continue_restored_chat(&self, scope: &Scope, record: &restore::Record, line: &str) -> bool {
        let Some(snapshot) = self
            .live
            .snapshot
            .as_deref()
            .filter(|_| self.live.status.is_connected())
        else {
            return false;
        };
        let current = self
            .endpoints
            .get(self.selected_endpoint)
            .is_some_and(|endpoint| endpoint.generation == scope.connection)
            && self.selection_epoch == scope.selection
            && snapshot.boot_id == scope.boot;
        current
            && free(&panes(snapshot), &record.pane)
            && self
                .type_script_line(&scope.boot, &record.pane, line)
                .is_ok()
    }
}
