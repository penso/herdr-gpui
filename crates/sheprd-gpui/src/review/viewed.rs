//! The files the user marked viewed in each checkout's review, kept across
//! restarts so a long review picks up where it was left. A mark keeps the
//! line counts the file had then, so a file that changes again comes back
//! unviewed, as it does within a session. Like worktree notes they belong to
//! this client alone and live in their own file, so a damaged one costs only
//! the marks.
use crate::state_file;
use gpui::{App, Global};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

/// Checkouts remembered; the least recently marked go first beyond this.
const MAX_CHECKOUTS: usize = 64;
/// Marks across every checkout; the oldest checkouts go first beyond this.
const MAX_FILES: usize = 4096;
/// Bytes in a path or a checkout's key part. A longer one is not remembered,
/// so every mark the store accepts fits in the file it reads back.
const MAX_FIELD_BYTES: usize = 1024;
/// Room for the most marks the store keeps, each as long as it accepts: JSON
/// escapes at most double a string's bytes, plus the counts and punctuation.
const MAX_FILE_BYTES: u64 = ((MAX_FILES + 2 * MAX_CHECKOUTS) * (2 * MAX_FIELD_BYTES + 64)) as u64;
const FILE: &str = "review-viewed.json";

/// Where a review's marks are kept: its checkout's directory, or its
/// repository when no directory is known, on its branch.
pub(crate) fn key(checkout: &crate::pull_request::Input) -> Option<(&str, &str)> {
    let place = checkout
        .checkout
        .as_deref()
        .or(checkout.repo_key.as_deref())?;
    Some((place, checkout.branch.as_str()))
}

/// A file's added and removed line counts, as the review diff reports them.
pub(crate) type Counts = (Option<u32>, Option<u32>);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Mark {
    path: String,
    added: Option<u32>,
    removed: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Checkout {
    /// The checkout's directory, or its repository when no directory is known.
    place: String,
    branch: String,
    files: Vec<Mark>,
}

impl Checkout {
    fn is(&self, place: &str, branch: &str) -> bool {
        self.place == place && self.branch == branch
    }

    fn valid(&self) -> bool {
        fits(&self.place)
            && fits(&self.branch)
            && !self.files.is_empty()
            && self.files.iter().all(|mark| fits(&mark.path))
    }
}

fn fits(field: &str) -> bool {
    !field.is_empty() && field.len() <= MAX_FIELD_BYTES
}

#[derive(Serialize, Deserialize)]
struct Saved {
    checkouts: Vec<Checkout>,
}

fn parse(bytes: &[u8]) -> crate::Result<Vec<Checkout>> {
    let saved: Saved = serde_json::from_slice(bytes)?;
    let marks: usize = saved.checkouts.iter().map(|c| c.files.len()).sum();
    if saved.checkouts.len() > MAX_CHECKOUTS
        || marks > MAX_FILES
        || !saved.checkouts.iter().all(Checkout::valid)
    {
        return Err(super::Error::InvalidViewed.into());
    }
    Ok(saved.checkouts)
}

/// The app's viewed marks, least recently marked checkout first.
#[derive(Default)]
pub(crate) struct Viewed {
    checkouts: Vec<Checkout>,
    writer: Option<state_file::Writer<Saved>>,
    quitting: bool,
}

impl Global for Viewed {}

impl Viewed {
    fn path() -> Option<PathBuf> {
        crate::preferences::state_dir().map(|dir| dir.join(FILE))
    }

    /// Called before starting GPUI, like worktree notes. A missing or
    /// damaged file starts with no marks.
    pub(crate) fn load() -> Self {
        Self::at(Self::path())
    }

    fn at(path: Option<PathBuf>) -> Self {
        let checkouts = path
            .as_deref()
            .map(|path| {
                state_file::read(path, MAX_FILE_BYTES)
                    .and_then(|bytes| bytes.as_deref().map_or(Ok(Vec::new()), parse))
            })
            .transpose()
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "Cannot restore the files marked viewed");
                None
            })
            .unwrap_or_default();
        let writer = path.and_then(
            |path| match state_file::Writer::start("review-viewed", path) {
                Ok(writer) => Some(writer),
                Err(error) => {
                    tracing::warn!(%error, "Cannot start the viewed files worker");
                    None
                }
            },
        );
        Self {
            checkouts,
            writer,
            quitting: false,
        }
    }

    pub(crate) fn install(self, cx: &mut App) {
        cx.set_global(self);
        cx.on_app_quit(|cx| {
            let viewed = cx.global_mut::<Self>();
            viewed.quitting = true;
            let writer = viewed.writer.take();
            cx.background_executor().spawn(async move {
                if let Some(writer) = writer {
                    writer.finish();
                }
            })
        })
        .detach();
    }

    /// The files marked viewed in `place` on `branch`, or none for fixtures
    /// that never installed the store.
    pub(crate) fn of(cx: &App, place: &str, branch: &str) -> HashMap<String, Counts> {
        cx.try_global::<Self>()
            .map(|viewed| viewed.get(place, branch))
            .unwrap_or_default()
    }

    /// Marks `path` viewed in `place` on `branch` at `counts`, for fixtures
    /// too, which keep it unsaved. Only that file changes, so reviews of one
    /// checkout in two windows keep each other's marks.
    pub(crate) fn remember(cx: &mut App, place: &str, branch: &str, path: &str, counts: Counts) {
        Self::installed(cx).mark(place, branch, path, counts);
    }

    /// Unmarks each of `marks` in `place` on `branch` that is still saved at
    /// the same counts; a file marked again since, in another window, stays.
    pub(crate) fn forget<'a>(
        cx: &mut App,
        place: &str,
        branch: &str,
        marks: impl IntoIterator<Item = (&'a str, Counts)>,
    ) {
        Self::installed(cx).unmark(place, branch, marks);
    }

    fn installed(cx: &mut App) -> &mut Self {
        if !cx.has_global::<Self>() {
            cx.set_global(Self::default());
        }
        cx.global_mut::<Self>()
    }

    fn mark(&mut self, place: &str, branch: &str, path: &str, counts: Counts) -> bool {
        let mut files = self.get(place, branch);
        files.insert(path.to_owned(), counts);
        self.set(place, branch, &files)
    }

    fn unmark<'a>(
        &mut self,
        place: &str,
        branch: &str,
        marks: impl IntoIterator<Item = (&'a str, Counts)>,
    ) -> bool {
        let mut files = self.get(place, branch);
        for (path, counts) in marks {
            if files.get(path) == Some(&counts) {
                files.remove(path);
            }
        }
        self.set(place, branch, &files)
    }

    fn get(&self, place: &str, branch: &str) -> HashMap<String, Counts> {
        self.checkouts
            .iter()
            .find(|checkout| checkout.is(place, branch))
            .map(|checkout| {
                checkout
                    .files
                    .iter()
                    .map(|mark| (mark.path.clone(), (mark.added, mark.removed)))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Replaces the checkout's marks with `files`, moving it to the most
    /// recent; no files forgets it. Returns whether anything changed.
    fn set(&mut self, place: &str, branch: &str, files: &HashMap<String, Counts>) -> bool {
        if !fits(place) || !fits(branch) {
            return false;
        }
        let mut marks: Vec<Mark> = files
            .iter()
            .filter(|(path, _)| fits(path))
            .map(|(path, (added, removed))| Mark {
                path: path.clone(),
                added: *added,
                removed: *removed,
            })
            .collect();
        // A stable order, so the same marks always write the same file.
        marks.sort_by(|a, b| a.path.cmp(&b.path));
        marks.truncate(MAX_FILES);
        let position = self
            .checkouts
            .iter()
            .position(|checkout| checkout.is(place, branch));
        if position.is_some_and(|index| self.checkouts[index].files == marks) {
            return false;
        }
        if let Some(index) = position {
            self.checkouts.remove(index);
        } else if marks.is_empty() {
            return false;
        }
        if !marks.is_empty() {
            self.checkouts.push(Checkout {
                place: place.to_owned(),
                branch: branch.to_owned(),
                files: marks,
            });
        }
        let mut total: usize = self.checkouts.iter().map(|c| c.files.len()).sum();
        while self.checkouts.len() > MAX_CHECKOUTS || total > MAX_FILES {
            total -= self.checkouts.remove(0).files.len();
        }
        self.save();
        true
    }

    fn save(&self) {
        if self.quitting {
            return;
        }
        if let Some(writer) = &self.writer {
            writer.save(Saved {
                checkouts: self.checkouts.clone(),
            });
        }
    }
}

#[cfg(test)]
mod tests;
