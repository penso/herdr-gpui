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
            made: self.made.clone(),
            finished: self.finished.clone(),
        }
    }
}

impl<T> LatestSave<T> {
    /// Makes `value` the choice the next save writes. The UI thread calls
    /// this; it never waits on a write in progress.
    pub(super) fn want(&self, value: T) {
        let mut wanted = self.wanted.lock().unwrap_or_else(PoisonError::into_inner);
        *wanted = Some(value);
        self.made.fetch_add(1, Ordering::SeqCst);
    }

    fn stamp(&self) -> Stamp {
        let made = self.made.load(Ordering::SeqCst);
        Stamp {
            made,
            current: self.finished.load(Ordering::SeqCst) == made,
        }
    }

    /// Whether a load stamped `stamp` holds the newest choice, rather than
    /// one the user has since moved on from.
    fn loaded_newest(&self, stamp: Stamp) -> bool {
        stamp.current && self.made.load(Ordering::SeqCst) == stamp.made
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

    /// Keeps `live`'s layouts in `loaded` where the load may hold an older
    /// choice. Saving a choice makes every window reload, so without this a
    /// quick second choice is undone by the reload of the first.
    pub(crate) fn keep_newer(&self, stamps: LayoutStamps, live: &Config, loaded: &mut Config) {
        if !self.chat.loaded_newest(stamps.chat) {
            loaded.chat.layout = live.chat.layout;
        }
        if !self.sidebar.loaded_newest(stamps.sidebar) {
            loaded.layout.mode = live.layout.mode;
        }
        if !self.tabs.loaded_newest(stamps.tabs) {
            loaded.tab_layout = live.tab_layout;
        }
    }
}

#[cfg(test)]
mod tests;
