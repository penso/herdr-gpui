//! A script's text, its name and the hash approval is pinned to. Runs and
//! plans load a fresh state from it each time.

use crate::{Error, Limits, Result, Script};
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path};

/// The largest script file read.
pub const MAX_SCRIPT_BYTES: u64 = 256 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub name: String,
    pub text: String,
    /// SHA-256 of the text, in lowercase hex.
    pub hash: String,
}

impl Source {
    pub fn new(name: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        let hash = Sha256::digest(text.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        Self {
            name: name.into(),
            text,
            hash,
        }
    }

    /// Reads a script file, named after its file stem.
    pub fn read(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let read = |source| Error::Read {
            path: path.to_owned(),
            source,
        };
        let file = fs::File::open(path).map_err(read)?;
        let size = file.metadata().map_err(read)?.len();
        if size > MAX_SCRIPT_BYTES {
            return Err(Error::TooLarge {
                size,
                limit: MAX_SCRIPT_BYTES,
            });
        }
        let mut text = String::new();
        file.take(MAX_SCRIPT_BYTES + 1)
            .read_to_string(&mut text)
            .map_err(read)?;
        let name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "script".to_owned());
        Ok(Self::new(name, text))
    }

    /// A fresh state with this script loaded.
    pub fn load(&self, limits: Limits) -> Result<Script> {
        Script::load(self.name.clone(), &self.text, limits)
    }
}
