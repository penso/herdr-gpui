//! Every origin this computer has served the app from. An installed app
//! keeps the origin it was installed from and calls whichever address
//! answers, so when a network change retires that origin it must still pass
//! CORS. Admitting a retired origin is safe only while a page there needs a
//! certificate from this computer's own authority, which only this computer
//! holds: the list is kept for that authority alone, named by its
//! fingerprint, and a user-supplied certificate remembers nothing.

use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

const FILE_NAME: &str = "origins.json";
/// The most origins kept; the oldest are forgotten first.
pub(crate) const MAX_ORIGINS: usize = 64;

#[derive(Debug, Default, Deserialize, Serialize)]
struct Stored {
    /// The authority whose certificates these origins were served with.
    fingerprint: String,
    /// Newest first.
    origins: Vec<String>,
}

#[derive(Debug)]
pub(crate) struct Origins {
    /// `None` when nothing may be remembered: a user-supplied certificate.
    path: Option<PathBuf>,
    stored: Stored,
}

impl Origins {
    /// Remembers nothing and admits nothing beyond the current origins.
    pub(crate) fn none() -> Self {
        Self {
            path: None,
            stored: Stored::default(),
        }
    }

    /// The origins remembered in `state_dir` for the authority with
    /// `fingerprint`; another authority's list, or an unreadable file, is
    /// none. Blocking.
    pub(crate) fn load(state_dir: &Path, fingerprint: &str) -> Self {
        let path = state_dir.join(FILE_NAME);
        let mut stored = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Stored>(&bytes).ok())
            .filter(|stored| stored.fingerprint == fingerprint)
            .unwrap_or_else(|| Stored {
                fingerprint: fingerprint.to_owned(),
                origins: Vec::new(),
            });
        stored.origins.truncate(MAX_ORIGINS);
        Self {
            path: Some(path),
            stored,
        }
    }

    pub(crate) fn contains(&self, origin: &str) -> bool {
        self.stored.origins.iter().any(|seen| seen == origin)
    }

    /// Remembers `current`, newest first, and saves when anything changed.
    /// Blocking.
    pub(crate) fn remember(&mut self, current: &[String]) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let mut origins: Vec<String> = current.to_vec();
        origins.extend(
            self.stored
                .origins
                .iter()
                .filter(|origin| !current.contains(origin))
                .cloned(),
        );
        origins.truncate(MAX_ORIGINS);
        if origins == self.stored.origins {
            return Ok(());
        }
        self.stored.origins = origins;
        let temp = path.with_extension("json.tmp");
        fs::write(&temp, serde_json::to_vec(&self.stored)?)?;
        fs::rename(&temp, path)
    }
}
