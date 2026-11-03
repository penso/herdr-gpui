//! The tabs a window keeps out of its strips, per workspace. Herdr lists a
//! workspace's tabs in every client and cannot move one to another
//! workspace, so a tab carried off to a new window is set apart here
//! instead: the new window hides the tabs it did not bring, and the one it
//! left hides the tab. Only this window's strips change; no tab closes, and
//! nothing is saved across restarts. A tab the window left hides comes
//! back once the window it went to closes.
use super::{Pick, Scope, layouts::Key};
use gpui::WindowId;
use std::collections::HashMap;

/// A hidden tab, and the window it was carried to, which gives it back on
/// closing. A window's own hidden tabs have none.
#[derive(Debug)]
struct Entry {
    pick: Pick,
    holder: Option<WindowId>,
}

#[derive(Debug, Default)]
pub(crate) struct Hidden(HashMap<Key, Vec<Entry>>);

impl Hidden {
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn hides(&self, key: &Key, pick: &Pick) -> bool {
        self.0
            .get(key)
            .is_some_and(|hidden| hidden.iter().any(|entry| entry.pick == *pick))
    }

    pub(crate) fn hide(&mut self, key: Key, picks: impl IntoIterator<Item = Pick>) {
        let hidden = self.0.entry(key).or_default();
        for pick in picks {
            if !hidden.iter().any(|entry| entry.pick == pick) {
                hidden.push(Entry { pick, holder: None });
            }
        }
        self.0.retain(|_, hidden| !hidden.is_empty());
    }

    /// Notes that `holder` shows the hidden `pick`, so closing it lists the
    /// tab here again.
    pub(crate) fn hold(&mut self, key: &Key, pick: &Pick, holder: WindowId) {
        if let Some(entry) = self
            .0
            .get_mut(key)
            .and_then(|hidden| hidden.iter_mut().find(|entry| entry.pick == *pick))
        {
            entry.holder = Some(holder);
        }
    }

    /// Lists `pick` of `key` again, as when the window it was to be carried
    /// to could not open.
    pub(crate) fn show(&mut self, key: &Key, pick: &Pick) {
        let Some(hidden) = self.0.get_mut(key) else {
            return;
        };
        hidden.retain(|entry| entry.pick != *pick);
        if hidden.is_empty() {
            self.0.remove(key);
        }
    }

    /// Lists again the tabs `closed` held. Returns whether there were any.
    pub(crate) fn release(&mut self, closed: WindowId) -> bool {
        let before: usize = self.0.values().map(Vec::len).sum();
        for hidden in self.0.values_mut() {
            hidden.retain(|entry| entry.holder != Some(closed));
        }
        self.0.retain(|_, hidden| !hidden.is_empty());
        self.0.values().map(Vec::len).sum::<usize>() != before
    }

    /// Forgets the hidden tabs of `key` that are no longer among `open`, so
    /// the set stays as small as the workspace.
    pub(crate) fn retain_open(&mut self, key: &Key, open: &[Pick]) {
        let Some(hidden) = self.0.get_mut(key) else {
            return;
        };
        hidden.retain(|entry| open.contains(&entry.pick));
        if hidden.is_empty() {
            self.0.remove(key);
        }
    }

    /// Forgets the workspaces the daemon closed.
    pub(crate) fn forget_workspaces(&mut self, scope: &Scope, closed: &[String]) {
        self.0
            .retain(|(saved, workspace), _| saved != scope || !closed.contains(workspace));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_tabs_are_kept_per_workspace_until_they_close() {
        let scope = Scope::endpoint("local");
        let key = (scope.clone(), "w1".to_owned());
        let other = (scope.clone(), "w2".to_owned());
        let (a, b) = (Pick::Herdr("a".into()), Pick::Herdr("b".into()));
        let mut hidden = Hidden::default();
        hidden.hide(key.clone(), [a.clone(), b.clone(), a.clone()]);
        hidden.hide(other.clone(), [a.clone()]);
        assert!(hidden.hides(&key, &a) && hidden.hides(&key, &b));
        assert_eq!(hidden.0[&key].len(), 2);

        hidden.retain_open(&key, std::slice::from_ref(&b));
        assert!(!hidden.hides(&key, &a) && hidden.hides(&key, &b));
        hidden.retain_open(&key, &[]);
        assert!(!hidden.0.contains_key(&key));

        hidden.forget_workspaces(&scope, &["w2".to_owned()]);
        assert!(!hidden.hides(&other, &a));
    }

    #[test]
    fn a_tab_shown_again_is_no_longer_hidden() {
        let key = (Scope::endpoint("local"), "w1".to_owned());
        let (a, b) = (Pick::Herdr("a".into()), Pick::Herdr("b".into()));
        let mut hidden = Hidden::default();
        hidden.hide(key.clone(), [a.clone(), b.clone()]);
        hidden.show(&key, &a);
        assert!(!hidden.hides(&key, &a) && hidden.hides(&key, &b));
        hidden.show(&key, &b);
        assert!(hidden.is_empty());
    }
}
