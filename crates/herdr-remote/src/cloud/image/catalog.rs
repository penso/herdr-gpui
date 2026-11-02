//! The images this app built, filed by project and provider, so Settings can
//! show them and Rebuild knows where the project's recipe is. It is a record,
//! not the truth: before reusing an image the provider is asked whether it
//! still holds it. One image per project and provider is kept.

use super::{Error, Field, Result, key::Project};
use crate::cloud::CloudProvider;
use std::{
    fs,
    io::{self, Read, Write},
    path::Path,
};

/// The image records file under the state directory.
pub const FILE: &str = "sandbox-images.json";
/// Room for every project's failed build log at its longest.
const LIMIT: u64 = 16 * 1024 * 1024;
const MAX_PROJECTS: usize = 500;
/// Log lines a failed build keeps for **Log**: its end, which says why.
pub const LOG_LINES: usize = 40;
/// Replaced images one record keeps while deleting them fails.
const MAX_STALE: usize = 16;

/// Where a project's recipe was last read: the endpoint, and the checkout's
/// path on it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Origin {
    pub endpoint_id: String,
    pub path: String,
}

/// How a project's latest build for one provider went.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Outcome {
    /// Built: what new machines start from.
    Ready { reference: String },
    /// The build failed, with the end of its log.
    Failed { log: Vec<String> },
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Image {
    pub provider: CloudProvider,
    pub key: String,
    /// Seconds since the Unix epoch.
    pub finished_at: u64,
    #[serde(flatten)]
    pub outcome: Outcome,
    /// Images earlier builds left at the provider, kept until deleting them
    /// works so a failed delete or a failed rebuild cannot orphan one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stale: Vec<String>,
}

impl Image {
    /// What new machines start from, when the build went well.
    fn reference(&self) -> Option<&str> {
        match &self.outcome {
            Outcome::Ready { reference } => Some(reference),
            Outcome::Failed { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    #[serde(flatten)]
    pub project: Project,
    pub origin: Origin,
    #[serde(default)]
    pub images: Vec<Image>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Document {
    version: u32,
    projects: Vec<Entry>,
}

/// The catalog file tests use instead of the user's, for every thread: this
/// crate's tests and the app's (`test-support`).
#[cfg(any(test, feature = "test-support"))]
pub static TEST_PATH: std::sync::Mutex<Option<std::path::PathBuf>> = std::sync::Mutex::new(None);

#[cfg(not(any(test, feature = "test-support")))]
fn path() -> Result<std::path::PathBuf> {
    herdr_client::local_path::state_dir()
        .map(|dir| dir.join(FILE))
        .ok_or(Error::StateDirectory)
}

fn io(path: &Path) -> impl FnOnce(io::Error) -> Error + '_ {
    move |source| Error::Catalog {
        path: path.to_owned(),
        source,
    }
}

fn read(path: &Path) -> Result<Vec<Entry>> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io(path)(error)),
    };
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(io(path))?;
    if bytes.len() as u64 > LIMIT {
        return Err(Error::Field(Field::Catalog));
    }
    let document: Document = serde_json::from_slice(&bytes).map_err(Error::Json)?;
    if document.version != 1 || document.projects.len() > MAX_PROJECTS {
        return Err(Error::Field(Field::Catalog));
    }
    Ok(document.projects)
}

fn write(path: &Path, projects: Vec<Entry>) -> Result<()> {
    let parent = path.parent().ok_or(Error::StateDirectory)?;
    fs::create_dir_all(parent).map_err(io(parent))?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(io(parent))?;
    serde_json::to_writer_pretty(
        &mut file,
        &Document {
            version: 1,
            projects,
        },
    )
    .map_err(Error::Json)?;
    file.write_all(b"\n").map_err(io(path))?;
    file.as_file().sync_all().map_err(io(path))?;
    file.persist(path).map_err(|error| io(path)(error.error))?;
    Ok(())
}

// Builds and windows each run their own workers; a read-modify-write must not
// interleave.
fn transaction<T>(work: impl FnOnce(&Path) -> Result<T>) -> Result<T> {
    static ACCESS: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = ACCESS.lock().unwrap_or_else(|error| error.into_inner());
    // Tests, here and the app's (`test-support`), never touch the user's
    // catalog: they use one only where a test set it.
    #[cfg(any(test, feature = "test-support"))]
    return work(
        &TEST_PATH
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .ok_or(Error::StateDirectory)?,
    );
    #[cfg(not(any(test, feature = "test-support")))]
    work(&path()?)
}

/// Every project with its images.
pub fn load() -> Result<Vec<Entry>> {
    transaction(read)
}

/// Record `image` as `project`'s latest for its provider, its recipe read
/// from `origin`. The image it replaces, and any still left from before,
/// stay in its `stale` list until [`deleted`] says they are gone; returns
/// that list, for the caller to delete.
pub fn record(project: &Project, origin: Origin, mut image: Image) -> Result<Vec<String>> {
    transaction(|path| {
        let mut projects = read(path)?;
        let index = match projects
            .iter()
            .position(|entry| entry.project.id == project.id)
        {
            Some(index) => index,
            None => {
                if projects.len() >= MAX_PROJECTS {
                    return Err(Error::Field(Field::Projects));
                }
                projects.push(Entry {
                    project: project.clone(),
                    origin: origin.clone(),
                    images: Vec::new(),
                });
                projects.len() - 1
            }
        };
        let entry = &mut projects[index];
        entry.project.label.clone_from(&project.label);
        entry.origin = origin;
        let slot = entry
            .images
            .iter()
            .position(|old| old.provider == image.provider);
        if let Some(old) = slot.map(|at| &entry.images[at]) {
            let current = image.reference().map(str::to_owned);
            for leftover in old.stale.iter().map(String::as_str).chain(old.reference()) {
                if current.as_deref() != Some(leftover)
                    && !image.stale.iter().any(|kept| kept == leftover)
                {
                    image.stale.push(leftover.to_owned());
                }
            }
            // Bounded: the oldest is forgotten rather than the file growing.
            let excess = image.stale.len().saturating_sub(MAX_STALE);
            for forgotten in image.stale.drain(..excess) {
                tracing::warn!(
                    category = "sandbox_image",
                    reference = %forgotten,
                    "Forgot a replaced image that could not be deleted"
                );
            }
        }
        let stale = image.stale.clone();
        match slot {
            Some(at) => entry.images[at] = image,
            None => entry.images.push(image),
        }
        write(path, projects)?;
        Ok(stale)
    })
}

/// Drop `gone`, images the provider deleted, from `project_id`'s stale list
/// for `provider`.
pub fn deleted(project_id: &str, provider: CloudProvider, gone: &[String]) -> Result<()> {
    if gone.is_empty() {
        return Ok(());
    }
    transaction(|path| {
        let mut projects = read(path)?;
        let Some(image) = projects
            .iter_mut()
            .filter(|entry| entry.project.id == project_id)
            .flat_map(|entry| entry.images.iter_mut())
            .find(|image| image.provider == provider)
        else {
            return Ok(());
        };
        image.stale.retain(|reference| !gone.contains(reference));
        write(path, projects)
    })
}

/// Forget `project`'s image from `provider`; returns it, for the caller to
/// delete. A project left with no image is forgotten too.
pub fn forget(project_id: &str, provider: CloudProvider) -> Result<Option<Image>> {
    transaction(|path| {
        let mut projects = read(path)?;
        let Some(index) = projects
            .iter()
            .position(|entry| entry.project.id == project_id)
        else {
            return Ok(None);
        };
        let entry = &mut projects[index];
        let removed = entry
            .images
            .iter()
            .position(|image| image.provider == provider)
            .map(|at| entry.images.remove(at));
        if entry.images.is_empty() {
            projects.remove(index);
        }
        write(path, projects)?;
        Ok(removed)
    })
}

/// Seconds since the Unix epoch, as images record.
pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

#[cfg(test)]
pub(crate) mod tests;
