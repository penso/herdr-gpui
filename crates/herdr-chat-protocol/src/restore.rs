//! What a chat leaves behind so a client can continue it once Herdr restores
//! its pane after a reboot or a server restart.
//!
//! Herdr restores a pane's layout and folder but not the program that ran in
//! it, nor the metadata token a chat announced its endpoint with. So while a
//! chat runs in a Herdr pane, `herdr-acp` keeps a [`Record`] of how to start
//! it again in [`restore_dir`]: the pane, the agent, the folder, and the
//! agent's session id. It removes the record when it ends on its own (the
//! user quit, or the agent ended); a record outlives only a chat that was
//! killed, as a reboot or a closed pane kills it.
//!
//! A client that sees the record's pane again, as a shell with no chat in
//! it, may continue the session there. The client also stamps each record
//! with the daemon boot it saw the chat live under ([`write_seen`]), so a
//! chat that died without its daemon restarting is never taken for a
//! restored one; and it claims a record ([`claim`]) before acting on it, so
//! of several windows or apps only one continues the chat.

mod error;

pub use error::Error;

use std::{
    fs,
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{Agent, EndpointId};

/// The record format this program writes and reads.
pub const VERSION: u32 = 1;
/// The most bytes a record or a seen stamp may hold.
pub const MAX_RECORD_SIZE: u64 = 8 * 1024;
/// Records [`list`] reads in one call; the rest wait for the next, which
/// starts where this one stopped.
pub const MAX_RECORDS: usize = 256;
/// Directory entries [`list`] and [`prune`] look at in one call.
pub const MAX_ENTRIES: usize = 4096;
/// The longest agent session id a record keeps.
pub const MAX_SESSION_ID: usize = 200;
/// The longest Herdr pane id a record keeps.
pub const MAX_PANE_ID: usize = 128;
/// The longest daemon boot id a seen stamp keeps.
pub const MAX_BOOT_ID: usize = 128;

/// How long a record no client ever saw live is kept: its chat ran while no
/// client was connected, and nothing can tell which boot it ran under.
pub const UNSEEN_AGE: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 60 * 60);
/// How long a claim may last before [`prune`] gives it back: longer than any
/// client holds one, so only a client that exited or crashed between claiming
/// and finishing leaves one this old. Also how long a temporary file lasts.
pub const CLAIM_STALE: std::time::Duration = std::time::Duration::from_secs(5 * 60);

const RECORD_EXTENSION: &str = "json";
const CLAIMED_EXTENSION: &str = "claimed";
const SEEN_EXTENSION: &str = "seen";

/// How to continue one chat in its restored pane.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Record {
    /// Whether the chat reached its agent through the agent's ACP adapter.
    pub adapter: bool,
    pub agent: Agent,
    /// The chat's folder, canonical.
    pub cwd: PathBuf,
    /// The endpoint the chat listened on, which names the record.
    pub endpoint: EndpointId,
    /// The Herdr pane the chat ran in.
    pub pane: String,
    /// The agent's session id, which `herdr-acp --resume` continues.
    pub session: String,
    /// The Herdr API socket the pane reported to (`HERDR_SOCKET_PATH`),
    /// which tells one Herdr session's panes from another's.
    pub socket: PathBuf,
}

/// The record as it is stored; [`Record`] is what a valid one reads as.
#[derive(Deserialize, Serialize)]
struct Stored {
    adapter: bool,
    agent: String,
    cwd: PathBuf,
    endpoint: String,
    pane: String,
    session: String,
    socket: PathBuf,
    version: u32,
}

impl From<&Record> for Stored {
    fn from(record: &Record) -> Self {
        Self {
            adapter: record.adapter,
            agent: record.agent.to_string(),
            cwd: record.cwd.clone(),
            endpoint: record.endpoint.to_string(),
            pane: record.pane.clone(),
            session: record.session.clone(),
            socket: record.socket.clone(),
            version: VERSION,
        }
    }
}

impl TryFrom<Stored> for Record {
    type Error = Error;

    fn try_from(stored: Stored) -> Result<Self, Error> {
        if stored.version != VERSION {
            return Err(Error::Version(stored.version));
        }
        let record = Self {
            adapter: stored.adapter,
            agent: stored.agent.parse().map_err(|_| Error::Agent)?,
            cwd: stored.cwd,
            endpoint: stored.endpoint.parse().map_err(|_| Error::Endpoint)?,
            pane: stored.pane,
            session: stored.session,
            socket: stored.socket,
        };
        record.validate()?;
        Ok(record)
    }
}

impl Record {
    /// Checks what a client types back into a shell: a session id and pane
    /// id of plain characters, and absolute paths.
    pub fn validate(&self) -> Result<(), Error> {
        if !session_id_is_plain(&self.session) {
            return Err(Error::Session);
        }
        if self.pane.is_empty()
            || self.pane.len() > MAX_PANE_ID
            || self.pane.chars().any(char::is_control)
        {
            return Err(Error::Pane);
        }
        if !self.cwd.is_absolute() || !self.socket.is_absolute() {
            return Err(Error::Path);
        }
        Ok(())
    }

    /// Writes the record into `dir`, replacing the chat's earlier one in a
    /// single rename, readable only by this user.
    pub fn write(&self, dir: &Path) -> Result<(), Error> {
        self.validate()?;
        let bytes = serde_json::to_vec(&Stored::from(self))?;
        if bytes.len() as u64 > MAX_RECORD_SIZE {
            return Err(Error::Size);
        }
        write_private(dir, &record_path(dir, &self.endpoint), &bytes)
    }

    /// Reads the record at `path`, at most [`MAX_RECORD_SIZE`] bytes.
    pub fn read(path: &Path) -> Result<Self, Error> {
        let bytes = read_bounded(path)?;
        Record::try_from(serde_json::from_slice::<Stored>(&bytes)?)
    }
}

/// Whether `id` is an agent session id a shell line may carry as one
/// argument: 1 to [`MAX_SESSION_ID`] letters, digits, and `._:@+-`, not
/// starting with `-` so it cannot read as an option.
pub fn session_id_is_plain(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_SESSION_ID
        && !id.starts_with('-')
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:@+-".contains(&byte))
}

/// The private directory restore records live in: `restore` beside the chat
/// sockets of [`crate::endpoint_dir`]. Windows keeps no sockets there, but
/// its records live in the same place under the home folder.
pub fn restore_dir() -> Option<PathBuf> {
    restore_dir_with(|name| std::env::var_os(name))
}

/// [`restore_dir`] with the environment supplied, for tests.
pub fn restore_dir_with(var: impl Fn(&str) -> Option<std::ffi::OsString>) -> Option<PathBuf> {
    let home = |name: &str| var(name).filter(|value| !value.is_empty());
    let dir = crate::endpoint_dir_with(|name| match name {
        // Windows names the home folder `USERPROFILE`.
        "HOME" => home("HOME").or_else(|| home("USERPROFILE")),
        name => home(name),
    })?;
    Some(dir.join("restore"))
}

pub fn record_path(dir: &Path, id: &EndpointId) -> PathBuf {
    dir.join(format!("{id}.{RECORD_EXTENSION}"))
}

pub fn claimed_path(dir: &Path, id: &EndpointId) -> PathBuf {
    dir.join(format!("{id}.{CLAIMED_EXTENSION}"))
}

pub fn seen_path(dir: &Path, id: &EndpointId) -> PathBuf {
    dir.join(format!("{id}.{SEEN_EXTENSION}"))
}

/// One call's worth of [`list`]: the records read, each with the boot a
/// client last saw its chat live under, and where the next call starts.
#[derive(Debug, Default)]
pub struct Listing {
    pub records: Vec<(Record, Option<String>)>,
    /// The last record read, when more remain after it; the next call
    /// passes it back to read on from there, round the directory.
    pub next: Option<EndpointId>,
    /// How many records the directory holds, read or not.
    pub total: usize,
}

impl Listing {
    /// Whether every record in the directory was read.
    pub fn complete(&self) -> bool {
        self.next.is_none()
    }
}

/// The endpoints of the records in `dir`, in order, from at most
/// [`MAX_ENTRIES`] entries.
fn record_ids(dir: &Path) -> Vec<EndpointId> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut ids: Vec<EndpointId> = entries
        .flatten()
        .take(MAX_ENTRIES)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == RECORD_EXTENSION)
        })
        .filter_map(|path| path.file_stem()?.to_str()?.parse().ok())
        .collect();
    ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    ids
}

/// At most [`MAX_RECORDS`] records in `dir`, in endpoint order, starting
/// after `after` and going round, so calls that pass back
/// [`Listing::next`] each read on and every record is reached in turn. A
/// record that does not read is skipped: it is another version's, or
/// damaged, and harms nobody.
pub fn list(dir: &Path, after: Option<&EndpointId>) -> Listing {
    let ids = record_ids(dir);
    let start = after.map_or(0, |after| {
        ids.iter()
            .position(|id| id.as_str() > after.as_str())
            .unwrap_or(0)
    });
    let count = ids.len().min(MAX_RECORDS);
    let window: Vec<&EndpointId> = ids.iter().cycle().skip(start).take(count).collect();
    let next = (ids.len() > MAX_RECORDS)
        .then(|| window.last().map(|id| (*id).clone()))
        .flatten();
    let records = window
        .into_iter()
        .filter_map(|id| Record::read(&record_path(dir, id)).ok())
        .map(|record| {
            let seen = read_seen(dir, &record.endpoint);
            (record, seen)
        })
        .collect();
    Listing {
        records,
        next,
        total: ids.len(),
    }
}

/// Stamps the record of `id`, when there is one, with the daemon boot its
/// chat was seen live under.
pub fn write_seen(dir: &Path, id: &EndpointId, boot: &str) -> Result<(), Error> {
    if boot.is_empty() || boot.len() > MAX_BOOT_ID || boot.chars().any(char::is_control) {
        return Err(Error::Boot);
    }
    if !record_path(dir, id).is_file() {
        return Ok(());
    }
    write_private(dir, &seen_path(dir, id), boot.as_bytes())
}

/// The boot the chat of `id` was last seen live under.
pub fn read_seen(dir: &Path, id: &EndpointId) -> Option<String> {
    let bytes = read_bounded(&seen_path(dir, id)).ok()?;
    String::from_utf8(bytes)
        .ok()
        .filter(|boot| !boot.is_empty() && boot.len() <= MAX_BOOT_ID)
}

/// Takes the record of `id` for this client in one rename, so of several
/// clients that see the same restored pane only one continues its chat.
/// `Ok(false)` when another client took it first or it is gone. The claim
/// is dated now, so [`prune`] can tell one a crashed client left behind.
pub fn claim(dir: &Path, id: &EndpointId) -> Result<bool, Error> {
    let claimed = claimed_path(dir, id);
    match fs::rename(record_path(dir, id), &claimed) {
        Ok(()) => {
            // Undated, a claim is given back after CLAIM_STALE from when
            // the record was written: sooner, never twice.
            let _ = fs::OpenOptions::new()
                .write(true)
                .open(&claimed)
                .and_then(|file| file.set_modified(std::time::SystemTime::now()));
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(Error::Io(error)),
    }
}

/// Gives a claimed record back, when the client could not act on it.
pub fn unclaim(dir: &Path, id: &EndpointId) -> Result<(), Error> {
    Ok(fs::rename(claimed_path(dir, id), record_path(dir, id))?)
}

/// Removes everything kept for the chat of `id`: its record, claimed or
/// not, and its seen stamp. Files already gone are not an error.
pub fn remove(dir: &Path, id: &EndpointId) {
    for path in [
        record_path(dir, id),
        claimed_path(dir, id),
        seen_path(dir, id),
    ] {
        let _ = fs::remove_file(path);
    }
}

/// Tidies `dir`, at most [`MAX_ENTRIES`] entries looked at: gives back
/// claims older than [`CLAIM_STALE`], which a client that exited or crashed
/// left, so a later pass can continue their chats; and removes records never
/// seen live for [`UNSEEN_AGE`], temporary files older than [`CLAIM_STALE`],
/// and seen stamps with no record. `now` is the wall clock to age against.
pub fn prune(dir: &Path, now: std::time::SystemTime) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let older_than = |path: &Path, age: std::time::Duration| {
        fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .is_ok_and(|modified| {
                now.duration_since(modified)
                    .is_ok_and(|elapsed| elapsed > age)
            })
    };
    let paths: Vec<PathBuf> = entries
        .flatten()
        .take(MAX_ENTRIES)
        .map(|entry| entry.path())
        .collect();
    for path in paths {
        let Some(id) = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| stem.parse::<EndpointId>().ok())
        else {
            continue;
        };
        match path.extension().and_then(|extension| extension.to_str()) {
            Some(RECORD_EXTENSION) => {
                if !seen_path(dir, &id).exists() && older_than(&path, UNSEEN_AGE) {
                    let _ = fs::remove_file(&path);
                }
            }
            Some(CLAIMED_EXTENSION) if older_than(&path, CLAIM_STALE) => {
                if record_path(dir, &id).exists() {
                    let _ = fs::remove_file(&path);
                } else {
                    let _ = fs::rename(&path, record_path(dir, &id));
                }
            }
            Some(SEEN_EXTENSION) => {
                if !record_path(dir, &id).exists() && !claimed_path(dir, &id).exists() {
                    let _ = fs::remove_file(&path);
                }
            }
            Some(extension) if extension.starts_with("tmp-") && older_than(&path, CLAIM_STALE) => {
                let _ = fs::remove_file(&path);
            }
            _ => {}
        }
    }
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, Error> {
    let file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(MAX_RECORD_SIZE + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_RECORD_SIZE {
        return Err(Error::Size);
    }
    Ok(bytes)
}

/// Writes `bytes` to `path` through a temporary file in `dir` and a rename,
/// with the directory and file private to this user on Unix.
fn write_private(dir: &Path, path: &Path, bytes: &[u8]) -> Result<(), Error> {
    create_private_dir(dir)?;
    // Unique per write, and never an existing file: two windows of one app
    // may stamp the same chat at once.
    static WRITES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let write = WRITES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut temporary = path.to_owned();
    temporary.set_extension(format!("tmp-{}-{write}", std::process::id()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let written = options
        .open(&temporary)
        .and_then(|mut file| file.write_all(bytes).and_then(|()| file.sync_all()))
        .and_then(|()| fs::rename(&temporary, path));
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    Ok(written?)
}

fn create_private_dir(dir: &Path) -> Result<(), Error> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    std::os::unix::fs::DirBuilderExt::mode(&mut builder, 0o700);
    builder.create(dir)?;
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
