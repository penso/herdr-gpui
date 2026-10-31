//! Lists the mods folder on a thread of its own, so the window learns about
//! new and removed mods without reading the disk itself.

use crate::system_load::Stop;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, PoisonError},
    thread,
    time::{Duration, Instant},
};

const INTERVAL: Duration = Duration::from_secs(2);
/// Mod folders listed at most, the first by name; `Mods` runs at most
/// [`super::MAX_MODS`] of those it does not turn off.
const MAX_LISTED: usize = 256;

pub(super) struct Scanner {
    stop: Arc<Stop>,
    names: Arc<Mutex<BTreeSet<String>>>,
}

impl Scanner {
    pub(super) fn spawn(dir: PathBuf) -> Self {
        let stop = Arc::new(Stop::default());
        let names = Arc::new(Mutex::new(BTreeSet::new()));
        let (thread_stop, thread_names) = (Arc::clone(&stop), Arc::clone(&names));
        let spawned = thread::Builder::new()
            .name("herdr-mods-scan".into())
            .spawn(move || {
                loop {
                    let found = list(&dir);
                    *thread_names.lock().unwrap_or_else(PoisonError::into_inner) = found;
                    if !thread_stop.wait(Instant::now() + INTERVAL) {
                        return;
                    }
                }
            });
        if let Err(error) = spawned {
            tracing::warn!(%error, "could not start the mods scanner");
        }
        Self { stop, names }
    }

    pub(super) fn names(&self) -> BTreeSet<String> {
        self.names
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Drop for Scanner {
    fn drop(&mut self) {
        self.stop.stop();
    }
}

/// Folders holding a `mod.toml`, the first [`MAX_LISTED`] by name whatever
/// order the directory lists them in; a missing folder lists none.
pub(super) fn list(dir: &Path) -> BTreeSet<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return BTreeSet::new();
    };
    let mut names = BTreeSet::new();
    for name in entries
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter(|entry| entry.path().join("mod.toml").is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !name.starts_with('.'))
    {
        names.insert(name);
        if names.len() > MAX_LISTED {
            names.pop_last();
        }
    }
    names
}
