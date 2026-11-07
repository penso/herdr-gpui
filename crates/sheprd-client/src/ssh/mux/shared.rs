//! The registry of this process's shared masters (see the parent module).
use super::{BACKOFF, IDLE, Lane, MAX_BACKOFF, READY_TIMEOUT};
use crate::control_dir::{ControlDir, control_path};
use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
    io,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, ExitStatus, Stdio},
    sync::{
        Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError, Weak,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

/// How often a waiting command and a starting master look again.
const POLL: Duration = Duration::from_millis(50);
/// How long a stopped master's watchdog may take to end it before it is
/// killed outright.
const STOP_GRACE: Duration = Duration::from_secs(5);

/// Runs the master (`"$@"`) and ends it once this shell's standard input, a
/// pipe from this process, closes; then removes the socket directory `$1`.
/// The pipe is copied to fd 3 first because a background job's own standard
/// input is `/dev/null`. The master gets SIGTERM, which lets it remove its
/// socket, then SIGKILL: `ssh` keeps SIGTERM ignored when it started with it
/// ignored, as a child of a GUI application can, and a shell cannot reset a
/// signal ignored on entry. The watcher inherits that ignored SIGTERM too, so
/// once the master exits on its own the shell stops the watcher with SIGKILL
/// and reaps it: a watcher left behind would wake when the pipe later closes
/// and signal the master's pid, which by then may be another process's. The
/// shell exits with the master's status.
const WATCHDOG: &str = r#"dir=$1
shift
exec 3<&0
"$@" </dev/null 3<&- &
master=$!
(
    read -r _ <&3
    kill "$master" 2>/dev/null
    sleep 1
    kill -9 "$master" 2>/dev/null
) &
watcher=$!
exec 3<&-
wait "$master"
status=$?
kill -9 "$watcher" 2>/dev/null
wait "$watcher" 2>/dev/null
rm -f "$dir/c"
rmdir "$dir" 2>/dev/null
exit "$status""#;

/// Why a master could not be started; the host then connects directly for a
/// while. Only logged.
#[derive(Debug, thiserror::Error)]
enum StartError {
    #[error("could not create the control socket directory")]
    Directory(#[source] io::Error),
    #[error("could not start ssh")]
    Spawn(#[source] io::Error),
    #[error("ssh exited before it was ready ({0})")]
    Exited(ExitStatus),
    #[error("ssh was not ready in time")]
    Timeout,
    #[error("the host has a master of the user's own")]
    UserMaster,
}

type Key = (String, Lane);

/// Whether a master is being started for a host, or recently failed to.
#[derive(Debug, Default, Clone, Copy)]
enum Phase {
    #[default]
    Free,
    Starting,
    Direct {
        until: Instant,
    },
}

#[derive(Debug, Default)]
struct Host {
    phase: Phase,
    /// Failed starts in a row, which lengthen the backoff.
    failures: u32,
    /// Bumped when the host's masters are retired, so a master that was
    /// starting at the time is not handed out once it is ready.
    generation: u64,
    /// At most one that is not retired; retired ones only drain.
    masters: Vec<Master>,
}

#[derive(Debug)]
struct Master {
    id: u64,
    socket: PathBuf,
    channels: usize,
    retired: bool,
    last_used: Instant,
    process: Process,
}

impl Master {
    fn exited(&mut self) -> bool {
        !self
            .process
            .child
            .as_mut()
            .is_some_and(|child| matches!(child.try_wait(), Ok(None)))
    }
}

/// A master's watchdog shell, the pipe that keeps it running, and its socket
/// directory. Dropping it stops the master and reaps it off this thread.
#[derive(Debug)]
struct Process {
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    dir: Option<ControlDir>,
}

impl Drop for Process {
    fn drop(&mut self) {
        // Closing the pipe has the watchdog end the master.
        drop(self.stdin.take());
        let (Some(mut child), dir) = (self.child.take(), self.dir.take()) else {
            return;
        };
        // Callers may hold the registry lock, so the wait is never here.
        let reap = move || {
            let deadline = Instant::now() + STOP_GRACE;
            while matches!(child.try_wait(), Ok(None)) && Instant::now() < deadline {
                thread::sleep(POLL);
            }
            let _ = child.kill();
            let _ = child.wait();
            drop(dir);
        };
        if let Err(error) = thread::Builder::new()
            .name("herdr-ssh-mux-stop".into())
            .spawn(reap)
        {
            tracing::warn!(category = "ssh_mux", %error, "Cannot reap a stopped SSH master");
        }
    }
}

/// The shared masters of this process, keyed by SSH target and lane.
#[derive(Debug)]
pub(crate) struct Registry {
    hosts: Mutex<HashMap<Key, Host>>,
    /// Each target and session a bridge has connected to; bounded by the
    /// saved devices.
    bridges: Mutex<HashSet<(String, String)>>,
    changed: Condvar,
    /// How long a master may sit with no channels before it is stopped.
    idle: Duration,
    /// Whether the thread that stops idle masters is running.
    janitor: AtomicBool,
    next: AtomicU64,
    /// `ssh`, or a stand-in under test.
    program: OsString,
}

pub(super) fn registry() -> &'static Arc<Registry> {
    static REGISTRY: OnceLock<Arc<Registry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Registry::new("ssh", IDLE))
}

/// One open channel on a master. Keep it until the command's child is
/// reaped: dropping it frees the master's slot.
#[derive(Debug)]
pub(crate) struct Channel {
    registry: Arc<Registry>,
    key: Key,
    id: u64,
    pub(super) socket: PathBuf,
}

impl Drop for Channel {
    fn drop(&mut self) {
        let mut hosts = self.registry.lock();
        let now = Instant::now();
        if let Some(master) = hosts
            .get_mut(&self.key)
            .and_then(|host| host.masters.iter_mut().find(|m| m.id == self.id))
        {
            master.channels = master.channels.saturating_sub(1);
            master.last_used = now;
        }
        sweep(&mut hosts, now, self.registry.idle);
        self.registry.changed.notify_all();
    }
}

impl Registry {
    pub(crate) fn new(program: impl Into<OsString>, idle: Duration) -> Arc<Self> {
        Arc::new(Self {
            hosts: Mutex::default(),
            bridges: Mutex::default(),
            changed: Condvar::new(),
            idle,
            janitor: AtomicBool::new(false),
            next: AtomicU64::new(1),
            program: program.into(),
        })
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<Key, Host>> {
        self.hosts.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// A channel on `target`'s `lane` master, starting one if there is none.
    /// `None` means connect directly: the master is full, failed recently, or
    /// was not ready by `deadline` or before `cancelled`.
    pub(crate) fn open(
        self: &Arc<Self>,
        target: &str,
        lane: Lane,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Option<Channel> {
        let key = (target.to_owned(), lane);
        let mut hosts = self.lock();
        loop {
            let now = Instant::now();
            sweep(&mut hosts, now, self.idle);
            let host = hosts.entry(key.clone()).or_default();
            if let Some(master) = host.masters.iter_mut().find(|m| !m.retired) {
                if master.channels >= lane.channels() {
                    return None;
                }
                master.channels += 1;
                master.last_used = now;
                return Some(Channel {
                    registry: self.clone(),
                    id: master.id,
                    socket: master.socket.clone(),
                    key,
                });
            }
            match host.phase {
                Phase::Direct { until } if now < until => return None,
                Phase::Starting => {}
                Phase::Free | Phase::Direct { .. } => {
                    host.phase = Phase::Starting;
                    if !self.start(key.clone(), host.generation) {
                        host.phase = Phase::Direct {
                            until: now + BACKOFF,
                        };
                        return None;
                    }
                }
            }
            if cancelled() {
                return None;
            }
            let left = deadline.checked_duration_since(now)?;
            hosts = self
                .changed
                .wait_timeout(hosts, left.min(POLL))
                .map_or_else(|error| error.into_inner().0, |(hosts, _)| hosts);
        }
    }

    /// See `super::bridge_connecting`.
    pub(crate) fn bridge_connecting(&self, target: &str, session: &str) {
        let first = self
            .bridges
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert((target.to_owned(), session.to_owned()));
        if !first {
            self.retire(target);
        }
    }

    /// Retire `target`'s masters in every lane: no new channels, and each
    /// stops once its last channel closes. One still starting is stopped as
    /// soon as it is ready.
    pub(crate) fn retire(&self, target: &str) {
        let mut hosts = self.lock();
        for ((host_target, _), host) in hosts.iter_mut() {
            if host_target == target {
                host.masters.iter_mut().for_each(|m| m.retired = true);
                host.generation = host.generation.wrapping_add(1);
            }
        }
        sweep(&mut hosts, Instant::now(), self.idle);
        self.changed.notify_all();
    }

    /// Starts `key`'s master on a thread of its own, so a waiting command can
    /// give up at its own deadline. False when no thread could be started.
    /// The master is dropped when the host was retired since `generation`.
    fn start(self: &Arc<Self>, key: Key, generation: u64) -> bool {
        let registry = self.clone();
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        thread::Builder::new()
            .name("herdr-ssh-mux".into())
            .spawn(move || {
                let started = launch(&registry.program, &key.0, key.1, id);
                let mut hosts = registry.lock();
                let host = hosts.entry(key.clone()).or_default();
                match started {
                    Ok(stale) if host.generation != generation => {
                        // A waiting command starts another on the host's
                        // current network path.
                        tracing::info!(category = "ssh_mux", lane = ?key.1, "SSH master retired while starting");
                        drop(stale);
                        host.phase = Phase::Free;
                    }
                    Ok(master) => {
                        tracing::info!(category = "ssh_mux", lane = ?key.1, "SSH master ready");
                        host.masters.push(master);
                        host.phase = Phase::Free;
                        host.failures = 0;
                        registry.watch_idle();
                    }
                    Err(error) => {
                        tracing::info!(
                            category = "ssh_mux",
                            lane = ?key.1,
                            %error,
                            "SSH master unavailable; connecting directly"
                        );
                        // The user's own master is no failed login: look
                        // again at the base interval.
                        if !matches!(error, StartError::UserMaster) {
                            host.failures = host.failures.saturating_add(1);
                        }
                        host.phase = Phase::Direct {
                            until: Instant::now() + backoff(host.failures),
                        };
                    }
                }
                registry.changed.notify_all();
            })
            .is_ok()
    }

    /// Starts the thread that stops idle masters unless it is running. The
    /// caller holds the registry lock, which orders this against the thread
    /// deciding to end.
    fn watch_idle(self: &Arc<Self>) {
        if self.janitor.swap(true, Ordering::AcqRel) {
            return;
        }
        let registry = Arc::downgrade(self);
        if let Err(error) = thread::Builder::new()
            .name("herdr-ssh-mux-idle".into())
            .spawn(move || stop_idle(&registry))
        {
            self.janitor.store(false, Ordering::Release);
            tracing::warn!(category = "ssh_mux", %error, "Cannot watch SSH masters for idleness");
        }
    }
}

/// Sweeps whenever the next master could have been idle long enough, until
/// none is left. Holds the registry only while sweeping, so dropping it is
/// never delayed by the wait.
fn stop_idle(registry: &Weak<Registry>) {
    loop {
        let Some(registry) = registry.upgrade() else {
            return;
        };
        let wait = {
            let mut hosts = registry.lock();
            let now = Instant::now();
            sweep(&mut hosts, now, registry.idle);
            let masters = || hosts.values().flat_map(|host| &host.masters);
            if masters().next().is_none() {
                registry.janitor.store(false, Ordering::Release);
                return;
            }
            // A master that goes idle later expires no sooner than `idle`
            // from now.
            masters()
                .filter(|master| master.channels == 0)
                .map(|master| (master.last_used + registry.idle).saturating_duration_since(now))
                .min()
                .unwrap_or(registry.idle)
                .min(registry.idle)
                .max(POLL)
        };
        drop(registry);
        thread::sleep(wait);
    }
}

/// Stops masters that exited, and ones with no channels that are retired or
/// have been unused for `idle`. Forgets hosts with nothing left to remember.
fn sweep(hosts: &mut HashMap<Key, Host>, now: Instant, idle: Duration) {
    for host in hosts.values_mut() {
        host.masters.retain_mut(|master| {
            if master.exited() {
                tracing::info!(category = "ssh_mux", "SSH master ended");
                return false;
            }
            master.channels > 0 || !(master.retired || now.duration_since(master.last_used) >= idle)
        });
    }
    hosts.retain(|_, host| !host.masters.is_empty() || !matches!(host.phase, Phase::Free));
}

/// `BACKOFF` doubled for each failure after the first, up to `MAX_BACKOFF`.
fn backoff(failures: u32) -> Duration {
    let doublings = failures.saturating_sub(1).min(16);
    BACKOFF.saturating_mul(1 << doublings).min(MAX_BACKOFF)
}

/// How long asking for the user's own master may take; it is local.
const CHECK_TIMEOUT: Duration = Duration::from_secs(5);

/// Starts a master and waits until it has authenticated, which is when it
/// creates its control socket. A host with a live master from the user's own
/// config gets none: commands connecting directly ride that one, which may
/// have been authenticated interactively where this master cannot be.
fn launch(program: &OsString, target: &str, lane: Lane, id: u64) -> Result<Master, StartError> {
    let mut check = crate::environment::command(&program.to_string_lossy());
    check.args(["-O", "check", "--", target]);
    let checked = crate::ssh::run(&mut check, CHECK_TIMEOUT, || false);
    if matches!(checked, Ok((status, _)) if status.success()) {
        return Err(StartError::UserMaster);
    }
    let dir = ControlDir::create("herdr-mux").map_err(StartError::Directory)?;
    let socket = dir.socket();
    let mut command = crate::environment::command("/bin/sh");
    command
        .args(["-c", WATCHDOG, "herdr-ssh-mux"])
        .arg(dir.path())
        .arg(program)
        .args(master_args(target, lane, &socket))
        .stdin(Stdio::piped())
        // Stderr can carry banners or secrets; nothing reads it.
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command.spawn().map_err(StartError::Spawn)?;
    let stdin = child.stdin.take();
    let mut master = Master {
        id,
        socket,
        channels: 0,
        retired: false,
        last_used: Instant::now(),
        process: Process {
            child: Some(child),
            stdin,
            dir: Some(dir),
        },
    };
    let deadline = Instant::now() + READY_TIMEOUT;
    loop {
        if let Some(Ok(Some(status))) = master.process.child.as_mut().map(Child::try_wait) {
            return Err(StartError::Exited(status));
        }
        if master.socket.exists() {
            master.last_used = Instant::now();
            return Ok(master);
        }
        if Instant::now() >= deadline {
            return Err(StartError::Timeout);
        }
        thread::sleep(POLL);
    }
}

/// The master's own connection policy: noninteractive like the bridge, a
/// master only for its private socket, and never a background process: the
/// watchdog follows the pid it started, so neither `ControlPersist` nor the
/// user's `ForkAfterAuthentication` may detach it. OpenSSH before 8.7 does
/// not know that option and refuses to start, before dialing, so such a
/// client connects directly as before. The quick lane compresses; the bulk
/// lane's data is often compressed already.
pub(super) fn master_args(target: &str, lane: Lane, socket: &Path) -> Vec<String> {
    let compression = match lane {
        Lane::Quick => "Compression=yes",
        Lane::Bulk => "Compression=no",
    };
    let mut args = vec!["-N".to_owned(), "-T".to_owned()];
    // The user's host key choice comes first, so it wins over the default.
    args.extend(
        crate::ssh::HostKeys::of(target)
            .options()
            .iter()
            .map(|&option| option.to_owned()),
    );
    for option in [
        "BatchMode=yes",
        "NumberOfPasswordPrompts=0",
        "StrictHostKeyChecking=yes",
        "ConnectTimeout=10",
        "ConnectionAttempts=1",
        "ServerAliveInterval=15",
        "ServerAliveCountMax=4",
        "ForwardX11=no",
        "ClearAllForwardings=yes",
        "ExitOnForwardFailure=no",
        "ControlMaster=yes",
        "ControlPersist=no",
        "ForkAfterAuthentication=no",
        compression,
        &control_path(socket),
    ] {
        args.push("-o".to_owned());
        args.push(option.to_owned());
    }
    args.extend(["--".to_owned(), target.to_owned()]);
    args
}

/// `remote_command` on `target` as a channel of the master on `socket`. Its
/// `ControlPath` comes before the shared policy, whose `ControlMaster=no`
/// keeps it a client; OpenSSH keeps the first value it reads. Should the
/// master be gone by the time it runs, SSH connects directly instead.
pub(super) fn channel_command(target: &str, remote_command: &str, socket: &Path) -> Command {
    let mut command = crate::environment::command("ssh");
    // Should the master be gone, the direct connection keeps its host's choice.
    command
        .args(crate::ssh::HostKeys::of(target).options())
        .arg("-o")
        .arg(control_path(socket))
        .args(crate::ssh::POLICY)
        .args(["-o", "ClearAllForwardings=yes", "--", target])
        .arg(remote_command);
    command
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
