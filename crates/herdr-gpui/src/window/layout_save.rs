//! Layout choices from the View menu, saved in the order they were made.

use crate::config::{LayoutMode, TabLayoutMode};
use std::sync::{Arc, Mutex, PoisonError};

/// The newest choice not yet written, and the lock its writer holds. Each
/// choice queues a save off the UI thread; saves write one at a time and
/// take whichever choice is newest when they start, so an earlier choice can
/// never land after a later one.
pub(super) struct LatestSave<T> {
    wanted: Arc<Mutex<Option<T>>>,
    writing: Arc<Mutex<()>>,
}

impl<T> Default for LatestSave<T> {
    fn default() -> Self {
        Self {
            wanted: Arc::default(),
            writing: Arc::default(),
        }
    }
}

impl<T> Clone for LatestSave<T> {
    fn clone(&self) -> Self {
        Self {
            wanted: self.wanted.clone(),
            writing: self.writing.clone(),
        }
    }
}

impl<T> LatestSave<T> {
    /// Makes `value` the choice the next save writes. The UI thread calls
    /// this; it never waits on a write in progress.
    pub(super) fn want(&self, value: T) {
        *self.wanted.lock().unwrap_or_else(PoisonError::into_inner) = Some(value);
    }

    /// Writes the newest choice, if one is still waiting, after any save
    /// already writing. Blocks, so it runs off the UI thread.
    pub(super) fn write_latest(
        &self,
        write: impl FnOnce(T) -> crate::Result<()>,
    ) -> crate::Result<()> {
        let _writing = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
        let Some(value) = self
            .wanted
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        else {
            return Ok(());
        };
        write(value)
    }
}

/// The saves every window shares, so choices made in different windows keep
/// their order too.
#[derive(Default)]
pub(super) struct LayoutSaves {
    pub(super) sidebar: LatestSave<LayoutMode>,
    pub(super) tabs: LatestSave<TabLayoutMode>,
}

impl gpui::Global for LayoutSaves {}

#[cfg(test)]
mod tests;
