use super::{Config, Error, server};
use crate::{browser::WebUrl, owned_process as process};
use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    process::{Child, Stdio},
    sync::{
        Arc, Mutex, PoisonError,
        mpsc::{self, Receiver, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Default)]
pub(crate) enum State {
    #[default]
    Idle,
    Starting,
    Ready(WebUrl),
    Failed(Arc<Error>),
}

pub(super) struct Worker {
    state: Arc<Mutex<State>>,
    stop: SyncSender<()>,
    done: Receiver<()>,
    owned: Arc<Mutex<Owned>>,
}

#[derive(Default)]
struct Owned {
    stopped: bool,
    child: Option<Child>,
}

fn cleanup(owned: &Mutex<Owned>) {
    let mut slot = owned.lock().unwrap_or_else(PoisonError::into_inner);
    slot.stopped = true;
    let child = slot.child.take();
    drop(slot);
    if let Some(mut child) = child {
        process::terminate(&child);
        thread::sleep(Duration::from_millis(50));
        process::kill(&mut child);
        let _ = child.wait();
    }
}

impl Worker {
    pub(super) fn start(config: Config, previous: Option<Self>) -> Result<Self, Error> {
        let state = Arc::new(Mutex::new(State::Starting));
        let (stop, stopped) = mpsc::sync_channel(1);
        let (finished, done) = mpsc::sync_channel(1);
        let output = state.clone();
        let owned = Arc::new(Mutex::new(Owned::default()));
        let child = owned.clone();
        thread::Builder::new()
            .name("herdr-px0".into())
            .spawn(move || {
                // Discovery must not attach to a workbench whose old worker
                // has taken its child out of `owned` but is still killing it.
                let result = previous
                    .map_or(Ok(()), Self::finish)
                    .and_then(|()| run(&config, &stopped, &output, &child));
                if let Err(error) = result {
                    *output.lock().unwrap_or_else(PoisonError::into_inner) =
                        State::Failed(Arc::new(error));
                }
                cleanup(&child);
                let _ = finished.send(());
            })
            .map_err(Error::Worker)?;
        Ok(Self {
            state,
            stop,
            done,
            owned,
        })
    }

    pub(super) fn state(&self) -> State {
        self.state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    pub(super) fn shutdown(self) {
        let _ = self.stop.try_send(());
        cleanup(&self.owned);
        let _ = self.done.recv_timeout(Duration::from_millis(20));
    }

    /// Replacement runs on a worker, so it can await full cleanup rather than
    /// use the short app-quit budget. Fail closed if completion is unavailable.
    fn finish(self) -> Result<(), Error> {
        let _ = self.stop.try_send(());
        cleanup(&self.owned);
        self.done
            .recv_timeout(Duration::from_secs(15))
            .map_err(Error::Shutdown)
    }

    pub(super) fn stop(&self) {
        let _ = self.stop.try_send(());
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.stop.try_send(());
    }
}

fn cancelled(stop: &Receiver<()>, wait: Duration) -> bool {
    !matches!(
        stop.recv_timeout(wait),
        Err(mpsc::RecvTimeoutError::Timeout)
    )
}

fn running(path: &Path) -> Option<server::Discovery> {
    server::discover(path)
        .ok()
        .flatten()
        .filter(|found| server::probe(&found.url).is_ok())
}

fn user_lock() -> Option<PathBuf> {
    std::env::var_os("XDG_STATE_HOME")
        .filter(|value| !value.is_empty())
        .map(|dir| PathBuf::from(dir).join("px0/workbench.lock"))
        .or_else(|| {
            crate::config::home()
                .ok()
                .map(|home| home.join(".px0/workbench.lock"))
        })
}

fn program(path: Option<&OsStr>) -> Option<PathBuf> {
    let name = if cfg!(windows) { "px0.exe" } else { "px0" };
    path.into_iter()
        .flat_map(std::env::split_paths)
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(name))
        .find(|path| {
            let Ok(metadata) = path.metadata() else {
                return false;
            };
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            }
            #[cfg(windows)]
            metadata.is_file()
        })
}

fn run(
    config: &Config,
    stop: &Receiver<()>,
    output: &Mutex<State>,
    owned: &Mutex<Owned>,
) -> Result<(), Error> {
    let url = if let Some(url) = &config.url {
        server::probe(url)?;
        url.clone()
    } else if let Some(existing) = user_lock().as_deref().and_then(running) {
        existing.url
    } else {
        // A private workbench registry avoids racing a user's bare `px0`
        // launch; its settings still come from their normal HOME.
        let state = crate::preferences::state_dir()
            .ok_or(Error::StateDirectory)?
            .join("px0-state");
        let lock = state.join("px0/workbench.lock");
        if let Some(existing) = running(&lock) {
            existing.url
        } else {
            let path = crate::login_env::path();
            let program = program(path.as_deref()).ok_or(Error::Missing)?;
            let Some(url) = launch(&program, &state, path.as_deref(), stop, owned)? else {
                return Ok(());
            };
            url
        }
    };
    *output.lock().unwrap_or_else(PoisonError::into_inner) = State::Ready(url.clone());
    while !cancelled(stop, Duration::from_secs(3)) {
        // A slow indexer or a transient connection failure must not destroy
        // an editor's in-memory edits or terminate a healthy owned workbench.
        let state = match server::probe(&url) {
            Ok(()) => State::Ready(url.clone()),
            Err(error) => State::Failed(Arc::new(error)),
        };
        *output.lock().unwrap_or_else(PoisonError::into_inner) = state;
    }
    Ok(())
}

fn launch(
    program: &Path,
    state: &Path,
    path: Option<&OsStr>,
    stop: &Receiver<()>,
    owned: &Mutex<Owned>,
) -> Result<Option<WebUrl>, Error> {
    if cancelled(stop, Duration::ZERO) {
        return Ok(None);
    }
    let mut command = sheprd_client::new_command(program);
    command
        .args([
            "-no-open",
            "-no-update",
            "-no-color",
            "-host",
            "127.0.0.1",
            "-port",
            "0",
            "-base-path",
            "/",
        ])
        .env("XDG_STATE_HOME", state);
    process::isolate(&mut command);
    if let Some(path) = path {
        command.env("PATH", path);
    }
    command.stdout(Stdio::null());
    let pid = {
        let mut slot = owned.lock().unwrap_or_else(PoisonError::into_inner);
        if slot.stopped {
            return Ok(None);
        }
        let child = command.spawn().map_err(Error::Spawn)?;
        let pid = child.id();
        slot.child = Some(child);
        pid
    };
    let lock = state.join("px0/workbench.lock");
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if cancelled(stop, Duration::from_millis(100)) {
            return Ok(None);
        }
        let exited = {
            let mut slot = owned.lock().unwrap_or_else(PoisonError::into_inner);
            if slot.stopped {
                return Ok(None);
            }
            slot.child
                .as_mut()
                .map(process::exited)
                .transpose()
                .map_err(Error::Process)?
                .unwrap_or(true)
        };
        if let Some(found) = server::discover(&lock)?
            && (found.pid == pid || exited)
            && server::probe(&found.url).is_ok()
        {
            return Ok(Some(found.url));
        }
        if exited {
            return Err(Error::Exited);
        }
        if Instant::now() >= deadline {
            return Err(Error::Timeout);
        }
    }
}

#[cfg(test)]
mod tests;
