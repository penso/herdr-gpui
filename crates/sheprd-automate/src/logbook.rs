//! The daemon's own log: JSON lines, rotated at a fixed size so it never
//! grows without bound. Sheprd's logs window reads it through
//! `herdr-automate log --daemon`.

use serde_json::json;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

const ROTATE_AT: u64 = 10 * 1024 * 1024;
const KEEP: u32 = 5;

#[derive(Clone, Debug)]
pub struct Logbook {
    path: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
    Error,
}

impl Level {
    fn name(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

impl Logbook {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn info(&self, message: impl AsRef<str>) {
        self.write(Level::Info, message.as_ref());
    }

    pub fn warn(&self, message: impl AsRef<str>) {
        self.write(Level::Warn, message.as_ref());
    }

    pub fn error(&self, message: impl AsRef<str>) {
        self.write(Level::Error, message.as_ref());
    }

    /// Best effort: the log must never stop the daemon.
    fn write(&self, level: Level, message: &str) {
        if let Some(parent) = self.path.parent() {
            let _ = crate::paths::private_dir(parent);
        }
        if fs::metadata(&self.path).is_ok_and(|metadata| metadata.len() >= ROTATE_AT) {
            rotate(&self.path);
        }
        let line = json!({
            "time": jiff::Timestamp::now().to_string(),
            "level": level.name(),
            "message": message,
        });
        if let Ok(mut file) = crate::paths::private_file()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = writeln!(file, "{line}");
        }
    }
}

/// `daemon.log` becomes `daemon.log.1`, and so on up to [`KEEP`].
fn rotate(path: &Path) {
    let numbered = |index: u32| {
        let mut name = path.as_os_str().to_owned();
        name.push(format!(".{index}"));
        PathBuf::from(name)
    };
    let _ = fs::remove_file(numbered(KEEP));
    for index in (1..KEEP).rev() {
        let _ = fs::rename(numbered(index), numbered(index + 1));
    }
    let _ = fs::rename(path, numbered(1));
}
