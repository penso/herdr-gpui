//! The rows picked with Shift or Cmd/Ctrl click, which the page's batch
//! actions then apply to. The selection is this page's alone: it is not
//! saved, and rows that leave the active list leave it too.

use super::Key;

#[derive(Debug, Default)]
pub(crate) struct Selection {
    keys: Vec<Key>,
    /// Where a Shift click's range starts: the row last toggled.
    anchor: Option<Key>,
}

impl Selection {
    pub(crate) fn contains(&self, key: &Key) -> bool {
        self.keys.contains(key)
    }

    pub(crate) fn len(&self) -> usize {
        self.keys.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    pub(crate) fn clear(&mut self) {
        self.keys.clear();
        self.anchor = None;
    }

    /// Adds `key`, or takes it out if it was selected.
    pub(crate) fn toggle(&mut self, key: &Key) {
        match self.keys.iter().position(|selected| selected == key) {
            Some(index) => {
                self.keys.remove(index);
            }
            None => self.keys.push(key.clone()),
        }
        self.anchor = Some(key.clone());
    }

    /// Adds the rows of `order` from the anchor to `key`, keeping what was
    /// selected. With no anchor still listed, it adds `key` alone.
    pub(crate) fn extend<'a>(&mut self, key: &Key, order: impl IntoIterator<Item = &'a Key>) {
        let order: Vec<_> = order.into_iter().collect();
        let end = order.iter().position(|row| *row == key);
        let start = self
            .anchor
            .as_ref()
            .and_then(|anchor| order.iter().position(|row| *row == anchor));
        let Some((start, end)) = start.zip(end) else {
            return self.toggle(key);
        };
        for row in &order[start.min(end)..=start.max(end)] {
            if !self.keys.contains(row) {
                self.keys.push((*row).clone());
            }
        }
    }

    /// Keeps only the rows still in `order`. Says whether any left.
    pub(crate) fn retain<'a>(&mut self, order: impl IntoIterator<Item = &'a Key>) -> bool {
        let order: Vec<_> = order.into_iter().collect();
        let before = self.keys.len();
        self.keys.retain(|key| order.contains(&key));
        if self
            .anchor
            .as_ref()
            .is_some_and(|anchor| !self.keys.contains(anchor))
        {
            self.anchor = None;
        }
        self.keys.len() != before
    }
}
