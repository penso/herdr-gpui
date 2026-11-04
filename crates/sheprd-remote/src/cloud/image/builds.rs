//! Builds running now, shared by the whole app: a dispatch and the Settings
//! card can ask for the same image, and both then wait on, and show, one
//! build. A build runs on the worker that started it; others block until it
//! ends. Views follow builds through [`watch`], which wakes on any change.

use super::{
    Error, Recipe, Support,
    catalog::{self, Image, Outcome},
    key::Project,
    progress::Progress,
};
use crate::cloud::CloudProvider;
use std::{
    collections::VecDeque,
    sync::{Condvar, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

/// A build running now, as views show it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Running {
    pub provider: CloudProvider,
    pub project: Project,
    pub key: String,
    pub progress: Progress,
    /// The end of its log, for **View log** while it runs.
    pub log: VecDeque<String>,
}

#[derive(Default)]
struct State {
    /// Bumped on every change, so a view redraws only for one.
    revision: u64,
    /// Bumped when the catalog changes, so a view rereads it only then
    /// rather than for every line a build prints.
    catalog: u64,
    running: Vec<Running>,
}

static STATE: Mutex<State> = Mutex::new(State {
    revision: 0,
    catalog: 0,
    running: Vec::new(),
});
/// Log lines a running build keeps for **Log**.
const LOG_LINES: usize = 200;
/// The longest log line kept, so a build printing long lines cannot swell
/// the catalog.
const LINE_LIMIT: usize = 300;
static CHANGED: Condvar = Condvar::new();

fn state() -> MutexGuard<'static, State> {
    STATE.lock().unwrap_or_else(PoisonError::into_inner)
}

fn changed(state: &mut State) {
    state.revision += 1;
    CHANGED.notify_all();
}

/// The builds running now, and the revision they are at.
pub fn snapshot() -> (u64, Vec<Running>) {
    let state = state();
    (state.revision, state.running.clone())
}

/// The catalog's revision: it changes when an image is recorded or reset.
pub fn catalog_revision() -> u64 {
    state().catalog
}

/// Wait until something changes after `revision`, or `timeout` passes.
/// Blocks: run it on a worker.
pub fn watch(revision: u64, timeout: Duration) -> (u64, Vec<Running>) {
    let state = state();
    let (state, _) = CHANGED
        .wait_timeout_while(state, timeout, |state| state.revision == revision)
        .unwrap_or_else(PoisonError::into_inner);
    (state.revision, state.running.clone())
}

/// Removes the build from the running list however it ends.
struct Registered {
    provider: CloudProvider,
    project_id: String,
}

impl Drop for Registered {
    fn drop(&mut self) {
        let mut state = state();
        state.running.retain(|running| {
            !(running.provider == self.provider && running.project.id == self.project_id)
        });
        changed(&mut state);
    }
}

/// Claim `project`'s build slot on `provider`, shown to views with `status`.
/// The caller has checked no one holds it.
fn register(
    state: &mut State,
    provider: CloudProvider,
    project: &Project,
    key: &str,
    status: &str,
) -> Registered {
    state.running.push(Running {
        provider,
        project: project.clone(),
        key: key.to_owned(),
        progress: Progress {
            step: None,
            status: status.into(),
        },
        log: VecDeque::new(),
    });
    changed(state);
    Registered {
        provider,
        project_id: project.id.clone(),
    }
}

/// Delete each of `stale`, the images a project's record keeps for
/// cleanup, and drop from the record the ones that went. Stops at the
/// first failure when `strict`, returning it; otherwise logs and goes on.
fn delete_stale(
    provider: CloudProvider,
    support: &Support,
    project_id: &str,
    stale: &[String],
    strict: bool,
) -> crate::Result<()> {
    let mut gone = Vec::new();
    let mut failure = None;
    for reference in stale {
        match (support.delete)(reference, &|| false) {
            Ok(()) => gone.push(reference.clone()),
            Err(error) if strict => {
                failure = Some(error);
                break;
            }
            Err(error) => {
                tracing::warn!(category = "sandbox_image", %error, "Could not delete a replaced image");
            }
        }
    }
    catalog::deleted(project_id, provider, &gone)?;
    failure.map_or(Ok(()), Err)
}

/// Take a line of the build's output; returns its progress after it.
fn update(provider: CloudProvider, project_id: &str, line: &str) -> Option<Progress> {
    let mut state = state();
    let running = state
        .running
        .iter_mut()
        .find(|running| running.provider == provider && running.project.id == project_id)?;
    for line in line.lines() {
        running.progress.line(line);
        if running.log.len() == LOG_LINES {
            running.log.pop_front();
        }
        running.log.push_back(
            line.chars()
                .filter(|c| !c.is_control())
                .take(LINE_LIMIT)
                .collect(),
        );
    }
    let progress = running.progress.clone();
    changed(&mut state);
    Some(progress)
}

/// The image `recipe` makes on `provider`, built unless the provider already
/// holds one with the same key; returns what new machines start from. A
/// build of the same project already running is waited for first. Blocks:
/// run it on a worker.
pub fn ensure(
    provider: CloudProvider,
    support: &Support,
    recipe: &Recipe,
    cancelled: &dyn Fn() -> bool,
    report: &dyn Fn(&Progress),
) -> crate::Result<String> {
    if !support.context && !recipe.files.is_empty() {
        return Err(Error::Context(crate::cloud::name(provider)).into());
    }
    let platform = (support.platform)(cancelled)?;
    let key = super::key::key(&recipe.dockerfile, &recipe.files, &platform);
    let name = (support.name)(&recipe.project, &key);
    let busy = |state: &State| {
        state
            .running
            .iter()
            .any(|running| running.provider == provider && running.project.id == recipe.project.id)
    };
    let registered = loop {
        {
            let mut state = state();
            while busy(&state) {
                if cancelled() {
                    return Err(Error::Cancelled.into());
                }
                state = CHANGED
                    .wait_timeout(state, Duration::from_millis(500))
                    .unwrap_or_else(PoisonError::into_inner)
                    .0;
            }
        }
        // Built already, here or by the build just waited for. Asked with
        // the lock released: views read it, and the provider may be slow.
        if let Some(reference) = reusable(provider, support, recipe, &key, &name, cancelled)? {
            return Ok(reference);
        }
        let mut state = state();
        // Another worker started one meanwhile: wait for it instead.
        if busy(&state) {
            continue;
        }
        break register(
            &mut state,
            provider,
            &recipe.project,
            &key,
            "Starting the build…",
        );
    };
    let log = |line: &str| {
        if let Some(progress) = update(provider, &recipe.project.id, line) {
            report(&progress);
        }
    };
    let built = (support.build)(&name, recipe, cancelled, &log);
    let tail = |extra: Option<String>| {
        let mut log: Vec<String> = snapshot()
            .1
            .into_iter()
            .find(|running| running.provider == provider && running.project.id == recipe.project.id)
            .map(|running| running.log.into())
            .unwrap_or_default();
        log.extend(extra);
        // The end says why; the catalog keeps only that much.
        let excess = log.len().saturating_sub(catalog::LOG_LINES);
        log.drain(..excess);
        log
    };
    let outcome = match &built {
        Ok(reference) => Outcome::Ready {
            reference: reference.clone(),
        },
        Err(error) => Outcome::Failed {
            log: tail(Some(error.to_string())),
        },
    };
    let stale = catalog::record(
        &recipe.project,
        recipe.origin.clone(),
        Image {
            provider,
            key,
            finished_at: catalog::now(),
            outcome,
            stale: Vec::new(),
        },
    );
    state().catalog += 1;
    let reference = built?;
    // Still holding the slot: a request rebuilding a replaced recipe waits,
    // so it is never handed an image deleted after it returned.
    delete_stale(provider, support, &recipe.project.id, &stale?, false)?;
    state().catalog += 1;
    drop(registered);
    Ok(reference)
}

/// The image `name` when the catalog records it built with `key` and the
/// provider still holds it.
fn reusable(
    provider: CloudProvider,
    support: &Support,
    recipe: &Recipe,
    key: &str,
    name: &str,
    cancelled: &dyn Fn() -> bool,
) -> crate::Result<Option<String>> {
    let recorded = catalog::load()?
        .into_iter()
        .find(|entry| entry.project.id == recipe.project.id)
        .and_then(|entry| {
            entry
                .images
                .into_iter()
                .find(|image| image.provider == provider && image.key == key)
        });
    if !matches!(
        recorded,
        Some(Image {
            outcome: Outcome::Ready { .. },
            ..
        })
    ) {
        return Ok(None);
    }
    (support.find)(name, recipe, cancelled)
}

/// Delete `project_id`'s image on `provider`, and those earlier builds left,
/// then forget it, so the next dispatch builds afresh. A build running for
/// it is left to finish. A failed delete keeps the record, so Reset can be
/// tried again.
pub fn reset(provider: CloudProvider, support: &Support, project_id: &str) -> crate::Result<()> {
    let project = catalog::load()?
        .into_iter()
        .find(|entry| entry.project.id == project_id)
        .map(|entry| entry.project);
    let _registered = {
        let mut state = state();
        let busy = state
            .running
            .iter()
            .any(|running| running.provider == provider && running.project.id == project_id);
        if busy {
            return Err(Error::Busy.into());
        }
        let Some(project) = project else {
            return Ok(());
        };
        register(&mut state, provider, &project, "", "Deleting the image…")
    };
    // Read again holding the slot, so a build that ended meanwhile is seen.
    let Some(image) = catalog::load()?
        .into_iter()
        .filter(|entry| entry.project.id == project_id)
        .flat_map(|entry| entry.images)
        .find(|image| image.provider == provider)
    else {
        return Ok(());
    };
    let mut doomed = image.stale.clone();
    // The current image goes last: until then the record still names it.
    if let Outcome::Ready { reference } = &image.outcome {
        doomed.push(reference.clone());
    }
    let deleted = delete_stale(provider, support, project_id, &doomed, true);
    if deleted.is_ok() {
        catalog::forget(project_id, provider)?;
    }
    let mut state = state();
    state.catalog += 1;
    changed(&mut state);
    deleted
}

#[cfg(test)]
mod tests;
