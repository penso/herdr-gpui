//! Layout choices from the View menu, saved in the order they were made.

use crate::config::{ChatLayoutMode, Config, LayoutMode, TabLayoutMode};
use std::sync::{
    Arc, Mutex, PoisonError,
    atomic::{AtomicU64, Ordering},
};

/// The newest choice not yet written, and the lock its writer holds. Each
/// choice queues a save off the UI thread; saves write one at a time and
/// take whichever choice is newest when they start, so an earlier choice can
/// never land after a later one.
pub(super) struct LatestSave<T> {
    wanted: Arc<Mutex<Option<T>>>,
    writing: Arc<Mutex<()>>,
    /// The last choice made in any window, which writing does not clear.
    newest: Arc<Mutex<Option<T>>>,
    /// Choices made so far, and the newest of them a save has finished with.
    /// A failed write counts as finished: nothing more is coming for it.
    made: Arc<AtomicU64>,
    finished: Arc<AtomicU64>,
}

/// What a config load could have read of one layout, taken as it starts.
#[derive(Clone, Copy, Debug)]
struct Stamp {
    made: u64,
    /// Every choice had reached the file, so the load can only read the
    /// newest. A save finishing later does not help: the load may have read
    /// the file before it.
    current: bool,
}

impl<T> Default for LatestSave<T> {
    fn default() -> Self {
        Self {
            wanted: Arc::default(),
            writing: Arc::default(),
            newest: Arc::default(),
            made: Arc::default(),
            finished: Arc::default(),
        }
    }
}

impl<T> Clone for LatestSave<T> {
    fn clone(&self) -> Self {
        Self {
            wanted: self.wanted.clone(),
            writing: self.writing.clone(),
            newest: self.newest.clone(),
            made: self.made.clone(),
            finished: self.finished.clone(),
        }
    }
}

impl<T: Copy> LatestSave<T> {
    /// Makes `value` the choice the next save writes. The UI thread calls
    /// this; it never waits on a write in progress.
    pub(super) fn want(&self, value: T) {
        let mut wanted = self.wanted.lock().unwrap_or_else(PoisonError::into_inner);
        *wanted = Some(value);
        *self.newest.lock().unwrap_or_else(PoisonError::into_inner) = Some(value);
        self.made.fetch_add(1, Ordering::SeqCst);
    }

    fn stamp(&self) -> Stamp {
        let made = self.made.load(Ordering::SeqCst);
        Stamp {
            made,
            current: self.finished.load(Ordering::SeqCst) == made,
        }
    }

    /// The newest choice, when a load stamped `stamp` may hold an older one.
    fn newer_than(&self, stamp: Stamp) -> Option<T> {
        if stamp.current && self.made.load(Ordering::SeqCst) == stamp.made {
            return None;
        }
        *self.newest.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Writes the newest choice, if one is still waiting, after any save
    /// already writing. Blocks, so it runs off the UI thread.
    pub(super) fn write_latest(
        &self,
        write: impl FnOnce(T) -> crate::Result<()>,
    ) -> crate::Result<()> {
        let _writing = self.writing.lock().unwrap_or_else(PoisonError::into_inner);
        let (value, made) = {
            let mut wanted = self.wanted.lock().unwrap_or_else(PoisonError::into_inner);
            let Some(value) = wanted.take() else {
                return Ok(());
            };
            (value, self.made.load(Ordering::SeqCst))
        };
        let result = write(value);
        self.finished.store(made, Ordering::SeqCst);
        result
    }
}

/// The saves every window shares, so choices made in different windows keep
/// their order too.
#[derive(Default)]
pub(crate) struct LayoutSaves {
    pub(super) chat: LatestSave<ChatLayoutMode>,
    pub(super) sidebar: LatestSave<LayoutMode>,
    pub(super) tabs: LatestSave<TabLayoutMode>,
}

impl gpui::Global for LayoutSaves {}

/// [`LayoutSaves`] as a config load starts.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LayoutStamps {
    chat: Stamp,
    sidebar: Stamp,
    tabs: Stamp,
}

impl LayoutSaves {
    pub(crate) fn stamp(&self) -> LayoutStamps {
        LayoutStamps {
            chat: self.chat.stamp(),
            sidebar: self.sidebar.stamp(),
            tabs: self.tabs.stamp(),
        }
    }

    /// Puts the newest choice in `loaded` where the load may hold an older
    /// one. Saving a choice makes every window reload, so without this a quick
    /// second choice is undone by the reload of the first. It is the shared
    /// choice, not a window's own: a window that did not make it may have just
    /// read it, and no later reload would bring it back.
    pub(crate) fn keep_newer(&self, stamps: LayoutStamps, loaded: &mut Config) {
        if let Some(mode) = self.chat.newer_than(stamps.chat) {
            loaded.chat.layout = mode;
        }
        if let Some(mode) = self.sidebar.newer_than(stamps.sidebar) {
            loaded.layout.mode = mode;
        }
        if let Some(mode) = self.tabs.newer_than(stamps.tabs) {
            loaded.tab_layout = mode;
        }
    }
}

#[cfg(test)]
mod tests;
