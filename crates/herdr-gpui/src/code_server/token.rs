//! The connection token of the VS Code server the app starts: made once,
//! kept in a file only its owner can read, and handed to the server by that
//! file's path, never on its command line. It is never logged or shown.
use super::{Error, error::Result};
use std::{
    fmt,
    fs::{self, OpenOptions},
    io::{ErrorKind, Read, Write},
    path::{Path, PathBuf},
};

const FILE: &str = "vscode-token";
/// Random bytes in a new token, written as twice as many hex digits.
const BYTES: usize = 32;
/// Longer than any token this app writes, so a stray file is not read whole.
const MAX_LEN: usize = 256;

pub(crate) struct Token(String);

impl Token {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }

    /// The token kept at `path`, made and written there first if there is
    /// none, or what is there is no token. Another instance of the app may
    /// do the same at once, so the whole of it holds an exclusive lock on a
    /// file beside the token: both then use the one token the first wrote,
    /// and neither replaces the other's.
    pub(super) fn load_or_create(path: &Path) -> Result<Self> {
        let failed = |source| Error::TokenFile {
            path: path.to_owned(),
            source,
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(failed)?;
        }
        let lock_path = path.with_extension("lock");
        let lock_failed = |source| Error::TokenFile {
            path: lock_path.clone(),
            source,
        };
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        let lock = options.open(&lock_path).map_err(lock_failed)?;
        lock.lock().map_err(lock_failed)?;
        // Released when `lock` is dropped, on every return below.
        if let Some(token) = read(path).map_err(failed)? {
            restrict(path).map_err(failed)?;
            return Ok(token);
        }
        let token = Self::random()?;
        // A file that is there but holds no token is replaced.
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(failed(error)),
        }
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        match options.open(path) {
            Ok(mut file) => {
                file.write_all(token.0.as_bytes()).map_err(failed)?;
                file.sync_all().map_err(failed)?;
                Ok(token)
            }
            Err(error) => Err(failed(error)),
        }
    }

    fn random() -> Result<Self> {
        let mut bytes = [0; BYTES];
        getrandom::fill(&mut bytes).map_err(Error::TokenRandom)?;
        Ok(Self(
            bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
        ))
    }

    /// What `code serve-web` takes as a token, which may also be written by
    /// hand: letters, digits, `-`, and `_`.
    fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let allowed = |c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_';
        (!text.is_empty() && text.len() <= MAX_LEN && text.chars().all(allowed))
            .then(|| Self(text.to_owned()))
    }
}

/// The token is the server's only lock, so it is never printed.
impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Token(..)")
    }
}

/// Where the app keeps the token: beside its other state.
pub(super) fn path() -> Result<PathBuf> {
    crate::preferences::state_dir()
        .map(|dir| dir.join(FILE))
        .ok_or(Error::NoDataDir)
}

/// The token in the file at `path`, or `None` when there is no file or it
/// holds no token.
fn read(path: &Path) -> std::io::Result<Option<Token>> {
    let mut text = String::new();
    match fs::File::open(path) {
        Ok(file) => match file.take(MAX_LEN as u64 + 1).read_to_string(&mut text) {
            Ok(_) => Ok(Token::parse(&text)),
            Err(error) if error.kind() == ErrorKind::InvalidData => Ok(None),
            Err(error) => Err(error),
        },
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

/// Takes away what others may do with the file, in case it was copied or
/// made by hand with looser permissions.
fn restrict(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(path)?.permissions().mode();
        if mode & 0o077 != 0 {
            fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o700))?;
        }
    }
    #[cfg(windows)]
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests;
