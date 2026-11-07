//! This computer's tags on disk (`control::tags`), and `sheprd-remote-control
//! tags`, which Sheprd runs over SSH on another computer. A host may have the
//! program but no daemon running, so the file stands in for one.

use crate::control::{self, Request, Response, tags::ComputerTags};
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

const FILE: &str = "tags.json";

/// Where the tags of the daemon whose state is `state_dir` are kept.
pub fn file(state_dir: &Path) -> PathBuf {
    state_dir.join(FILE)
}

/// The tags saved in `file`, bounded; none when it is missing or unreadable,
/// as a damaged file costs only labels. Blocking.
pub fn load(file: &Path) -> ComputerTags {
    fs::read(file)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<ComputerTags>(&bytes).ok())
        .map(|saved| ComputerTags::new(saved.tags, saved.updated_ms))
        .unwrap_or_default()
}

/// Replaces `file` whole, so a crash leaves the old tags or the new ones.
/// Blocking.
pub fn save(file: &Path, tags: &ComputerTags) -> super::Result<()> {
    let failed = |operation, source| super::Error::Io {
        operation,
        path: file.to_owned(),
        source,
    };
    let bytes =
        serde_json::to_vec(tags).map_err(|error| failed("encode", io::Error::from(error)))?;
    let partial = file.with_extension("json.partial");
    super::enroll::write_private(&partial, &bytes)?;
    fs::rename(&partial, file).map_err(|source| failed("replace", source))
}

/// Runs `work` as the only process changing `file`: with no daemon running,
/// each `tags --offer` is its own process, and one that read an older copy
/// could otherwise finish last. The system frees the lock if its holder dies.
/// Blocking.
pub fn locked<T>(file: &Path, work: impl FnOnce() -> super::Result<T>) -> super::Result<T> {
    let path = file.with_extension("lock");
    let failed = |operation, source| super::Error::Io {
        operation,
        path: path.clone(),
        source,
    };
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(|source| failed("open", source))?;
    lock.lock().map_err(|source| failed("lock", source))?;
    work()
}

/// Offers `offer` to the tags in `state_dir` and returns those that stand:
/// through the daemon when one runs, so its phones hear of it at once, else
/// in the file it reads when it starts. Blocking.
pub fn exchange(state_dir: &Path, offer: Option<ComputerTags>) -> super::Result<ComputerTags> {
    let address = control::address(state_dir);
    let request = Request::Tags {
        offer: offer.clone(),
    };
    match control::request(&address, &request) {
        Ok(Response::Tags(tags)) => Ok(tags),
        Ok(_) => Err(super::Error::Unexpected),
        Err(control::Error::Connect(_)) => {
            let file = file(state_dir);
            let Some(offer) = offer else {
                return Ok(load(&file));
            };
            fs::create_dir_all(state_dir).map_err(|source| super::Error::Io {
                operation: "create",
                path: state_dir.to_owned(),
                source,
            })?;
            locked(&file, || {
                let saved = load(&file);
                let merged = saved.clone().merged(offer, now_ms());
                if merged != saved {
                    save(&file, &merged)?;
                }
                Ok(merged)
            })
        }
        Err(error) => Err(error.into()),
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
