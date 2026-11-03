//! Continuing chats in the panes Herdr restores after a reboot or a server
//! restart. Herdr brings a pane back as a shell in its folder, without the
//! `herdr-acp` that ran there or the token it announced; the program left a
//! record of how to continue it (`herdr_chat_protocol::restore`), and this
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
//! - Herdr says the pane's shell is at its prompt, in the record's folder;
//! - this window claimed the record first, so of several windows or apps
//!   only one continues the chat.
//!
//! The pass runs a few times, [`PASS_INTERVAL`] apart, for each daemon boot
//! and connection, since a restored shell may still be starting when the
//! first snapshot arrives; its last run also drops records whose pane is
//! gone. It runs only for a daemon on this machine whose `herdr` CLI can be
//! asked about a pane's processes (Linux and macOS).

use super::{Route, discovery};
use crate::{HerdrWindow, listening_ports, processes};
use herdr_chat_protocol::{EndpointId, TOKEN, restore};
use herdr_client::protocol::ClientShellSnapshot;
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    sync::{atomic::AtomicBool, mpsc},
    time::{Duration, Instant},
};

/// How often live chats are stamped with the boot they run under.
const STAMP_INTERVAL: Duration = Duration::from_secs(5);
/// How long the restore pass waits between its runs for one scope.
const PASS_INTERVAL: Duration = Duration::from_secs(5);
/// How many times the restore pass runs for one scope.
const PASSES: u8 = 6;

/// The connection and daemon boot a pass belongs to.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Scope {
    boot: String,
    connection: u64,
    selection: u64,
}

/// A restored pane as the snapshot shows it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Pane {
    /// Its folder as the snapshot reports it, for when Herdr cannot say
    /// where its shell is.
    pub(super) cwd: Option<String>,
    pub(super) id: String,
    /// Whether an agent or a chat is reported in it.
    pub(super) occupied: bool,
}

/// What the window learned off the UI thread.
enum Outcome {
    /// Records this window claimed, each with the line that continues it.
    Claimed(Scope, Vec<(restore::Record, String)>),
    /// Live chats whose records now carry the current boot.
    Stamped(Scope, Vec<EndpointId>),
}

/// The window's restore state. One background job runs at a time.
#[derive(Default)]
pub(crate) struct Restore {
    job: Option<mpsc::Receiver<Outcome>>,
    /// When the restore pass may run again for [`Self::passes`]' scope.
    next_pass: Option<Instant>,
    next_stamp: Option<Instant>,
    /// The scope the restore pass ran for, and how many times.
    passes: Option<(Scope, u8)>,
    stamped: HashSet<EndpointId>,
    stamped_scope: Option<Scope>,
}

/// The panes of `snapshot`, each marked occupied when the daemon reports an
/// agent or a chat token in it.
pub(super) fn panes(snapshot: &ClientShellSnapshot) -> Vec<Pane> {
    let occupied: HashSet<&str> = snapshot
        .agents
        .iter()
        .filter(|agent| agent.agent.is_some() || agent.tokens.iter().any(|(name, _)| name == TOKEN))
        .map(|agent| agent.pane_id.as_str())
        .collect();
    snapshot
        .panes
        .iter()
        .map(|pane| Pane {
            cwd: pane.foreground_cwd.clone().or_else(|| pane.cwd.clone()),
            id: pane.pane_id.clone(),
            occupied: occupied.contains(pane.pane_id.as_str()),
        })
        .collect()
}

/// The records worth checking further for `boot`: seen live under another
/// boot, of this daemon (`owns` their socket), naming an unoccupied pane
/// that no other record names. Pure, so it is tested without files.
pub(super) fn plan(
    records: Vec<(restore::Record, Option<String>)>,
    panes: &[Pane],
    boot: &str,
    owns: impl Fn(&Path) -> bool,
) -> Vec<restore::Record> {
    let mut named: HashMap<String, usize> = HashMap::new();
    for (record, _) in &records {
        *named.entry(record.pane.clone()).or_default() += 1;
    }
    records
        .into_iter()
        .filter(|(_, seen)| seen.as_deref().is_some_and(|seen| seen != boot))
        .map(|(record, _)| record)
        .filter(|record| owns(&record.socket))
        // Two chats once in one pane: which is the latest cannot be told,
        // so neither is continued.
        .filter(|record| named.get(&record.pane) == Some(&1))
        .filter(|record| {
            panes
                .iter()
                .any(|pane| pane.id == record.pane && !pane.occupied)
        })
        .take(super::MAX_CHATS)
        .collect()
}

/// The line typed into a restored shell to continue `record` with
/// `program`. `sh` runs it, so it reads the same from bash, zsh, fish, or
/// nushell; each argument rides as its own single-quoted word, and a value
/// holding a quote or a control character makes no line at all. A leading
/// space keeps it out of shells' history that honor that.
pub(super) fn line(program: &Path, record: &restore::Record) -> Option<String> {
    let program = program.to_str()?;
    let mut words = vec![program, "--resume", record.session.as_str()];
    if record.adapter {
        words.push("--acp");
    }
    words.push(record.agent.name());
    if !restore::session_id_is_plain(&record.session)
        || words
            .iter()
            .any(|word| word.contains('\'') || word.chars().any(char::is_control))
    {
        return None;
    }
    let quoted: Vec<String> = words.iter().map(|word| format!("'{word}'")).collect();
    Some(format!(" sh -c 'exec \"$0\" \"$@\"' {}", quoted.join(" ")))
}

/// Whether a program still listens on `endpoint`. Only a refused or missing
/// socket counts as gone; anything else is taken as alive.
fn listening(endpoint: &EndpointId) -> bool {
    #[cfg(unix)]
    {
        let Some(address) = herdr_chat_protocol::endpoint_address(endpoint) else {
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
    let Ok(info) = daemon.process_info(&pane.id, &cancelled) else {
        return false;
    };
    if info.foreground_job().is_some() {
        return false;
    }
    let shell_cwd = info
        .foreground_processes
        .iter()
        .find(|process| Some(process.pid) == info.shell_pid)
        .and_then(|process| process.cwd.clone())
        .or_else(|| pane.cwd.clone());
    shell_cwd
        .and_then(|shell_cwd| std::fs::canonicalize(shell_cwd).ok())
        .is_some_and(|shell_cwd| shell_cwd == cwd)
}

/// The records of this daemon (`owns` their socket) that no restore can
/// use any more: seen live under an earlier boot, and their pane is gone.
/// A record never seen live may belong to a chat whose pane is newer than
/// the snapshot, so [`restore::prune`] ages those out instead.
pub(super) fn orphans<'a>(
    records: &'a [(restore::Record, Option<String>)],
    panes: &[Pane],
    boot: &str,
    owns: impl Fn(&Path) -> bool,
) -> impl Iterator<Item = &'a EndpointId> {
    records
        .iter()
        .filter(move |(record, seen)| {
            seen.as_deref().is_some_and(|seen| seen != boot)
                && owns(&record.socket)
                && !panes.iter().any(|pane| pane.id == record.pane)
        })
        .map(|(record, _)| &record.endpoint)
}

/// The restore pass, off the UI thread: the records [`plan`] keeps whose
/// endpoint is gone and whose pane is an idle shell in their folder, each
/// claimed for this window with its line. The `last` run also removes the
/// [`orphans`].
fn pass(
    dir: &Path,
    panes: &[Pane],
    boot: &str,
    owner: &listening_ports::Daemon,
    daemon: &processes::Daemon,
    program: &Path,
    last: bool,
) -> Vec<(restore::Record, String)> {
    let owns = |socket: &Path| socket.to_str().is_some_and(|socket| owner.owns(socket));
    let records = restore::list(dir);
    if last {
        for endpoint in orphans(&records, panes, boot, owns) {
            restore::remove(dir, endpoint);
        }
        restore::prune(dir, std::time::SystemTime::now());
    }
    plan(records, panes, boot, owns)
        .into_iter()
        .filter(|record| !listening(&record.endpoint))
        .filter(|record| {
            panes
                .iter()
                .find(|pane| pane.id == record.pane)
                .is_some_and(|pane| idle_in(daemon, pane, &record.cwd))
        })
        .filter_map(|record| Some((line(program, &record)?, record)))
        .filter(|(_, record)| restore::claim(dir, &record.endpoint).unwrap_or(false))
        .map(|(line, record)| (record, line))
        .collect()
}

/// Runs `job` on a thread of its own; the window reads its outcome on a
/// later tick. A thread that cannot start leaves nothing to read.
fn spawn(job: impl FnOnce() -> Outcome + Send + 'static) -> Option<mpsc::Receiver<Outcome>> {
    let (sender, receiver) = mpsc::sync_channel(1);
    std::thread::Builder::new()
        .name("chat-restore".to_owned())
        .spawn(move || {
            let _ = sender.send(job());
        })
        .ok()?;
    Some(receiver)
}

/// Runs `work` on a thread of its own and forgets it: removing or giving
/// back a claimed record needs no answer.
fn detach(work: impl FnOnce() + Send + 'static) {
    let _ = std::thread::Builder::new()
        .name("chat-restore".to_owned())
        .spawn(work);
}

impl HerdrWindow {
    /// Runs every tick while chats are on: runs the restore pass for the
    /// daemon's boot, continues the chats it claimed, and stamps live chats
    /// with the boot in between.
    pub(super) fn poll_chat_restore(&mut self, route: Option<&Route>) {
        let Some(dir) = restore::restore_dir() else {
            return;
        };
        self.finish_chat_restore(&dir);
        if self.chats.restore.job.is_some() || route != Some(&Route::Local) {
            return;
        }
        let Some(endpoint) = self.endpoints.get(self.selected_endpoint) else {
            return;
        };
        let target = &endpoint.connection.target;
        // A daemon whose socket cannot be named could own any record.
        let (Some(owner), Some(daemon)) = (
            listening_ports::Daemon::of(target).filter(listening_ports::Daemon::identified),
            processes::Daemon::for_target(target),
        ) else {
            return;
        };
        let Some(snapshot) = self
            .live
            .snapshot
            .as_deref()
            .filter(|_| self.live.status.is_connected())
        else {
            return;
        };
        let scope = Scope {
            boot: snapshot.boot_id.clone(),
            connection: endpoint.generation,
            selection: self.selection_epoch,
        };
        let now = Instant::now();
        let restore = &mut self.chats.restore;
        let passes = match &restore.passes {
            Some((ran, passes)) if *ran == scope => *passes,
            _ => 0,
        };
        let due = restore.next_pass.is_none_or(|next| now >= next) || passes == 0;
        if passes < PASSES
            && due
            && let Ok(program) = self.chats.program.found().map(Path::to_owned)
        {
            let panes = panes(snapshot);
            restore.passes = Some((scope.clone(), passes + 1));
            restore.next_pass = Some(now + PASS_INTERVAL);
            let last = passes + 1 == PASSES;
            restore.job = spawn(move || {
                let claimed = pass(&dir, &panes, &scope.boot, &owner, &daemon, &program, last);
                Outcome::Claimed(scope, claimed)
            });
            return;
        }
        let live: Vec<EndpointId> = snapshot
            .agents
            .iter()
            .filter_map(|agent| discovery::endpoint(&agent.tokens))
            .take(super::MAX_CHATS)
            .collect();
        self.stamp_live_chats(dir, scope, live, now);
    }

    /// Stamps the records of the `live` chats with the scope's boot, now and
    /// then, each once per scope.
    fn stamp_live_chats(
        &mut self,
        dir: PathBuf,
        scope: Scope,
        live: Vec<EndpointId>,
        now: Instant,
    ) {
        let restore = &mut self.chats.restore;
        if restore.next_stamp.is_some_and(|next| now < next) {
            return;
        }
        restore.next_stamp = Some(now + STAMP_INTERVAL);
        if restore.stamped_scope.as_ref() != Some(&scope) {
            restore.stamped.clear();
            restore.stamped_scope = Some(scope.clone());
        }
        let live: Vec<EndpointId> = live
            .into_iter()
            .filter(|endpoint| !restore.stamped.contains(endpoint))
            .collect();
        if live.is_empty() {
            return;
        }
        restore.job = spawn(move || {
            let stamped = live
                .into_iter()
                .filter(|endpoint| {
                    restore::write_seen(&dir, endpoint, &scope.boot).is_ok()
                        && restore::read_seen(&dir, endpoint).as_deref() == Some(&scope.boot)
                })
                .collect();
            Outcome::Stamped(scope, stamped)
        });
    }

    /// Applies a finished job: remembers stamped chats, and types the line
    /// into each claimed pane that is still as the pass found it.
    fn finish_chat_restore(&mut self, dir: &Path) {
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
            Outcome::Claimed(scope, claimed) => {
                for (record, line) in claimed {
                    let typed = self.continue_restored_chat(&scope, &record, &line);
                    let dir = dir.to_owned();
                    let endpoint = record.endpoint.clone();
                    if typed {
                        tracing::info!(pane = %record.pane, agent = %record.agent, "Continuing a chat in its restored pane");
                        detach(move || restore::remove(&dir, &endpoint));
                    } else {
                        detach(move || {
                            let _ = restore::unclaim(&dir, &endpoint);
                        });
                    }
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
        let free = panes(snapshot)
            .iter()
            .any(|pane| pane.id == record.pane && !pane.occupied);
        current
            && free
            && self
                .type_script_line(&scope.boot, &record.pane, line)
                .is_ok()
    }
}
