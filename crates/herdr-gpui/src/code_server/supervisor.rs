//! The `code serve-web` child the app starts for VS Code tabs. One
//! worker thread owns it: it makes the address (the token and the port),
//! checks that the port is free, starts the child, waits for VS Code to
//! answer, and starts it again with backoff whenever it stops. The UI thread
//! only reads its state and asks it to stop, never waiting for it.
//!
//! A supervisor exists only once the user accepted VS Code's server license
//! in the app, since the child is started with `--accept-server-license-terms`.
use super::{Error, Server, token::Token};
use crate::browser::WebUrl;
use std::{
    fmt,
    io::ErrorKind,
    net::{Ipv4Addr, TcpListener},
    num::NonZeroU16,
    path::PathBuf,
    process::{Child, Command, ExitStatus},
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, atomic::Ordering},
    thread,
    time::{Duration, Instant},
};

mod process;

/// What to start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Plan {
    pub(crate) program: PathBuf,
    /// The saved port; `None` picks a free one and saves it, since VS Code
    /// keeps its settings in the storage of the page's origin.
    pub(crate) port: Option<NonZeroU16>,
    pub(crate) token_file: PathBuf,
}

/// Where the server listens, and the address with its token that the
/// VS Code tab loads.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Address {
    pub(crate) port: NonZeroU16,
    pub(crate) url: WebUrl,
}

/// The address carries the token, so only the port is printed.
impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Address")
            .field("port", &self.port)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Debug)]
pub(crate) enum Status {
    /// Making the address, or the child runs and VS Code has not answered:
    /// the first start downloads the server, which takes a while.
    Starting,
    /// VS Code answered at the address.
    Ready,
    /// The last start failed, or VS Code stopped; it starts again at `retry`.
    Failed {
        error: Arc<crate::Error>,
        retry: Instant,
    },
}

/// The worker's pace; tests run it faster.
#[derive(Clone, Copy, Debug)]
pub(super) struct Timing {
    /// How often the worker looks at the child and, until VS Code answers,
    /// asks it.
    pub(super) poll: Duration,
    /// How long VS Code has to answer, download included.
    pub(super) ready_timeout: Duration,
    /// How long the child has to stop when asked, before it is killed.
    pub(super) grace: Duration,
    pub(super) backoff: Backoff,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            poll: Duration::from_millis(250),
            ready_timeout: Duration::from_secs(600),
            grace: Duration::from_secs(2),
            backoff: Backoff {
                first: Duration::from_secs(1),
                max: Duration::from_secs(60),
                healthy: Duration::from_secs(60),
            },
        }
    }
}

/// How long the worker waits before starting VS Code again.
#[derive(Clone, Copy, Debug)]
pub(super) struct Backoff {
    pub(super) first: Duration,
    pub(super) max: Duration,
    /// A server that answered for this long starts the count over.
    pub(super) healthy: Duration,
}

impl Backoff {
    /// The wait after `failures` failures in a row: doubling, up to `max`.
    pub(super) fn delay(&self, failures: u32) -> Duration {
        let doublings = failures.saturating_sub(1).min(16);
        self.first.saturating_mul(1 << doublings).min(self.max)
    }
}

#[derive(Default)]
struct Slot {
    stopped: bool,
    /// The worker is starting the child, which is not in `child` yet. A
    /// shutdown waits for it rather than finish with the child unseen.
    spawning: bool,
    child: Option<Child>,
    status: Option<Status>,
    address: Option<Address>,
    /// Counts changes to `status` and `address`, so a reader can tell.
    revision: u64,
    /// A stand-in worker reported Ready, so it counts as running.
    #[cfg(test)]
    stand_in_ready: bool,
}

#[derive(Default)]
struct Shared {
    slot: Mutex<Slot>,
    /// Wakes the worker's waits when it is stopped.
    wake: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, Slot> {
        self.slot.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn publish(&self, change: impl FnOnce(&mut Slot)) {
        let mut slot = self.lock();
        change(&mut slot);
        slot.revision += 1;
    }

    /// Waits `duration`, or less once stopped. Returns whether it stopped.
    fn sleep(&self, duration: Duration) -> bool {
        let deadline = Instant::now() + duration;
        let mut slot = self.lock();
        loop {
            if slot.stopped {
                return true;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            slot = self
                .wake
                .wait_timeout(slot, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    /// Whether the child runs right now, so it, and nothing else, holds its
    /// port. Quick: it looks without waiting.
    fn alive(&self) -> bool {
        let mut slot = self.lock();
        #[cfg(test)]
        if slot.child.is_none() && slot.stand_in_ready {
            return !slot.stopped;
        }
        !slot.stopped
            && slot
                .child
                .as_mut()
                .is_some_and(|child| matches!(process::exited(child), Ok(false)))
    }

    /// Asks the child to stop, gives it `grace` to exit, then kills what is
    /// left and reaps it. `None` when there is no child, or another caller
    /// is already ending it.
    fn end(&self, grace: Duration) -> Option<std::io::Result<ExitStatus>> {
        let deadline = Instant::now() + grace;
        {
            let mut slot = self.lock();
            while slot.spawning {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    break;
                }
                slot = self
                    .wake
                    .wait_timeout(slot, left)
                    .unwrap_or_else(PoisonError::into_inner)
                    .0;
            }
            process::terminate(slot.child.as_ref()?);
        }
        loop {
            {
                let mut slot = self.lock();
                let child = slot.child.as_mut()?;
                if process::exited(child).unwrap_or(true) {
                    break;
                }
            }
            if Instant::now() >= deadline {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let mut child = self.lock().child.take()?;
        process::kill(&mut child);
        Some(child.wait())
    }
}

/// The running worker. Dropping it stops the child without waiting.
pub(crate) struct Supervisor {
    shared: Arc<Shared>,
    plan: Plan,
}

/// What the worker last reported.
#[derive(Clone, Debug)]
pub(crate) struct Report {
    pub(crate) revision: u64,
    pub(crate) status: Status,
    pub(crate) address: Option<Address>,
}

impl Supervisor {
    pub(crate) fn start(plan: Plan) -> crate::Result<Self> {
        Self::start_with(
            plan,
            |plan, port| {
                let mut command = serve_web(plan, port);
                crate::login_env::apply(&mut command);
                command
            },
            // Never the token: the app wrote the file the server reads it from.
            super::version,
            |port| crate::config::Config::save_code(crate::config::CodeEdit::Port(port)),
            Timing::default(),
        )
    }

    pub(super) fn start_with(
        plan: Plan,
        command: impl Fn(&Plan, NonZeroU16) -> Command + Send + 'static,
        probe: impl Fn(&WebUrl) -> crate::Result<Server> + Send + 'static,
        save_port: impl Fn(NonZeroU16) -> crate::Result<()> + Send + 'static,
        timing: Timing,
    ) -> crate::Result<Self> {
        let shared = Arc::new(Shared::default());
        let worker = Worker {
            shared: shared.clone(),
            port: plan.port,
            plan: plan.clone(),
            command,
            probe,
            save_port,
            timing,
        };
        thread::Builder::new()
            .name("herdr-vscode".into())
            .spawn(move || worker.run())
            .map_err(Error::Worker)?;
        Ok(Self { shared, plan })
    }

    pub(crate) fn plan(&self) -> &Plan {
        &self.plan
    }

    pub(crate) fn report(&self) -> Report {
        let slot = self.shared.lock();
        Report {
            revision: slot.revision,
            status: slot.status.clone().unwrap_or(Status::Starting),
            address: slot.address.clone(),
        }
    }

    /// Whether the child runs right now: what a window checks just before
    /// it loads a page, which carries the token.
    pub(crate) fn alive(&self) -> bool {
        self.shared.alive()
    }

    /// Asks the child to stop and the worker to end, without waiting: the
    /// worker kills what is left after its grace and reaps it.
    pub(crate) fn stop(&self) {
        let mut slot = self.shared.lock();
        slot.stopped = true;
        if let Some(child) = &slot.child {
            process::terminate(child);
        }
        self.shared.wake.notify_all();
    }

    /// Stops the child, waits up to `budget` for it, then kills what is left
    /// and reaps it. Blocking: for quitting, off the UI thread.
    pub(crate) fn shutdown(&self, budget: Duration) {
        self.stop();
        self.shared.end(budget);
    }
}

/// A worker that runs nothing, for tests of what uses one.
#[cfg(test)]
impl Supervisor {
    pub(crate) fn idle(plan: Plan) -> crate::Result<Self> {
        Ok(Self {
            shared: Arc::default(),
            plan,
        })
    }

    /// Reports `address`, as the worker does once it has made it.
    #[cfg(all(test, any(target_os = "macos", windows)))]
    pub(crate) fn report_address(&self, address: Address) {
        self.shared.publish(|slot| slot.address = Some(address));
    }

    /// The stand-in's child exits, before the worker notices.
    #[cfg(all(test, any(target_os = "macos", windows)))]
    pub(crate) fn stand_in_exits(&self) {
        self.shared.lock().stand_in_ready = false;
    }

    /// Reports `status` and `address`, as the worker would.
    #[cfg(all(test, any(target_os = "macos", windows)))]
    pub(crate) fn report_as(&self, status: Status, address: Option<Address>) {
        self.shared.publish(|slot| {
            slot.stand_in_ready = matches!(status, Status::Ready);
            slot.status = Some(status);
            slot.address = address;
        });
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.stop();
    }
}

/// `code serve-web` on the loopback port, with the token file.
pub(super) fn serve_web(plan: &Plan, port: NonZeroU16) -> Command {
    let mut command = Command::new(&plan.program);
    command
        .args(["serve-web", "--host", "127.0.0.1", "--port"])
        .arg(port.to_string())
        .arg("--connection-token-file")
        .arg(&plan.token_file)
        .arg("--accept-server-license-terms");
    command
}

/// How one start ended.
enum Ended {
    Stopped,
    Failed { error: crate::Error, healthy: bool },
}

/// A start that failed before VS Code answered.
fn failed(error: impl Into<crate::Error>) -> Ended {
    Ended::Failed {
        error: error.into(),
        healthy: false,
    }
}

struct Worker<C, P, S> {
    shared: Arc<Shared>,
    plan: Plan,
    /// The port in use, once picked.
    port: Option<NonZeroU16>,
    command: C,
    probe: P,
    save_port: S,
    timing: Timing,
}

impl<C, P, S> Worker<C, P, S>
where
    C: Fn(&Plan, NonZeroU16) -> Command,
    P: Fn(&WebUrl) -> crate::Result<Server>,
    S: Fn(NonZeroU16) -> crate::Result<()>,
{
    fn run(mut self) {
        let mut failures = 0;
        loop {
            let (error, healthy) = match self.attempt() {
                Ended::Stopped => return,
                Ended::Failed { error, healthy } => (error, healthy),
            };
            failures = if healthy { 1 } else { failures + 1 };
            let delay = self.timing.backoff.delay(failures);
            // Errors name the host and port alone, never the token.
            tracing::info!(%error, ?delay, "VS Code stopped; starting it again");
            self.shared.publish(|slot| {
                slot.status = Some(Status::Failed {
                    error: Arc::new(error),
                    retry: Instant::now() + delay,
                });
            });
            if self.shared.sleep(delay) {
                return;
            }
        }
    }

    fn attempt(&mut self) -> Ended {
        if self.shared.lock().stopped {
            return Ended::Stopped;
        }
        let address = match self.prepare() {
            Ok(address) => address,
            Err(error) => return failed(error),
        };
        // Only to name the problem: whatever holds the port is never asked
        // anything, since every question carries the token. The child's own
        // word that it listens is what makes the port safe to ask.
        let port = address.port.get();
        match TcpListener::bind((Ipv4Addr::LOCALHOST, port)) {
            Ok(listener) => drop(listener),
            Err(error) if error.kind() == ErrorKind::AddrInUse => {
                return failed(Error::PortTaken { port });
            }
            Err(source) => return failed(Error::PortCheck { port, source }),
        }
        self.serve(&address)
    }

    /// The address for this start. The token is read again each time, so a
    /// token file removed meanwhile is made again; the port stays.
    fn prepare(&mut self) -> crate::Result<Address> {
        let token = Token::load_or_create(&self.plan.token_file)?;
        let port = match self.port {
            Some(port) => port,
            None => {
                let port = free_port()?;
                // VS Code still runs on it; the next launch picks another.
                if let Err(error) = (self.save_port)(port) {
                    tracing::warn!(%error, port = port.get(), "Could not save the VS Code port");
                }
                self.port = Some(port);
                port
            }
        };
        let url = WebUrl::try_from(format!("http://127.0.0.1:{port}/?tkn={}", token.as_str()))?;
        let address = Address { port, url };
        self.shared.publish(|slot| {
            slot.status = Some(Status::Starting);
            slot.address = Some(address.clone());
        });
        Ok(address)
    }

    fn serve(&self, address: &Address) -> Ended {
        {
            let mut slot = self.shared.lock();
            if slot.stopped {
                return Ended::Stopped;
            }
            slot.spawning = true;
        }
        let mut command = (self.command)(&self.plan, address.port);
        process::isolate(&mut command);
        let spawned = command.spawn();
        let stdout = {
            let mut slot = self.shared.lock();
            slot.spawning = false;
            let stdout = spawned.map(|mut child| {
                let stdout = child.stdout.take();
                slot.child = Some(child);
                stdout
            });
            self.shared.wake.notify_all();
            stdout
        };
        let stdout = match stdout {
            Ok(stdout) => stdout,
            Err(source) => {
                return failed(Error::Spawn {
                    program: self.plan.program.clone(),
                    source,
                });
            }
        };
        tracing::info!(port = address.port.get(), "Started VS Code");
        let listening = match stdout.map(process::listening) {
            Some(Ok(flag)) => flag,
            Some(Err(source)) => {
                self.shared.end(self.timing.grace);
                return failed(Error::Worker(source));
            }
            None => {
                self.shared.end(self.timing.grace);
                return failed(Error::Worker(ErrorKind::BrokenPipe.into()));
            }
        };
        let started = Instant::now();
        let mut answered: Option<Instant> = None;
        loop {
            let exited = {
                let mut slot = self.shared.lock();
                match (slot.stopped, slot.child.as_mut()) {
                    (false, Some(child)) => process::exited(child),
                    _ => {
                        drop(slot);
                        self.shared.end(self.timing.grace);
                        return Ended::Stopped;
                    }
                }
            };
            let healthy = answered.is_some_and(|at| at.elapsed() >= self.timing.backoff.healthy);
            match exited {
                Ok(false) => {}
                Ok(true) => {
                    // Its port is free again, for anyone: no window may
                    // send the token there meanwhile.
                    self.shared
                        .publish(|slot| slot.status = Some(Status::Starting));
                    let error = match self.shared.end(self.timing.grace) {
                        Some(Ok(status)) => Error::Exited(status),
                        Some(Err(source)) => Error::Watch(source),
                        // Taken by a shutdown, which stopped it.
                        None => return Ended::Stopped,
                    };
                    return Ended::Failed {
                        error: error.into(),
                        healthy,
                    };
                }
                Err(source) => {
                    self.shared.end(self.timing.grace);
                    return failed(Error::Watch(source));
                }
            }
            if answered.is_none() {
                // Alive before the question and after the answer: a live
                // serve-web holds its port, so the answer was its own.
                if listening.load(Ordering::Acquire)
                    && (self.probe)(&address.url).is_ok()
                    && self.shared.alive()
                {
                    answered = Some(Instant::now());
                    self.shared
                        .publish(|slot| slot.status = Some(Status::Ready));
                } else if started.elapsed() >= self.timing.ready_timeout {
                    self.shared.end(self.timing.grace);
                    return failed(Error::Silent(self.timing.ready_timeout));
                }
            }
            self.shared.sleep(self.timing.poll);
        }
    }
}

/// A loopback port the system considers free right now.
fn free_port() -> Result<NonZeroU16, Error> {
    let port = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .map_err(Error::NoFreePort)?;
    NonZeroU16::new(port).ok_or_else(|| Error::NoFreePort(ErrorKind::AddrNotAvailable.into()))
}

#[cfg(test)]
mod tests;
