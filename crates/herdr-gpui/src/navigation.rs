//! Where a focus request is aimed. A closed set of targets, so navigation is
//! never a boolean-plus-ID pair or a string tag.

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NavigationTarget<T> {
    Workspace(T),
    Tab(T),
    Pane(T),
}

pub(crate) type OwnedNavigationTarget = NavigationTarget<String>;

impl crate::HerdrWindow {
    /// The newest requested destination, for presentation only. The daemon's
    /// snapshot remains authoritative for commands and terminal input.
    pub(crate) fn navigation_preview(&self) -> Option<&OwnedNavigationTarget> {
        self.pending_navigation.as_ref().or_else(|| {
            self.live
                .activation
                .as_ref()
                .filter(|activation| !activation.failed && activation.active)
                .and_then(|activation| activation.focus.as_ref())
        })
    }

    pub(crate) fn workspace_selected(
        &self,
        endpoint: usize,
        workspace: &herdr_client::protocol::ClientShellWorkspace,
    ) -> bool {
        if endpoint != self.selected_endpoint {
            return false;
        }
        let requested = self.navigation_preview().and_then(|target| {
            let snapshot = self.live.snapshot.as_ref()?;
            match target.as_deref() {
                NavigationTarget::Workspace(id) => Some(id),
                NavigationTarget::Tab(id) => snapshot
                    .tabs
                    .iter()
                    .find(|tab| tab.tab_id == id)
                    .map(|tab| tab.workspace_id.as_str()),
                NavigationTarget::Pane(id) => snapshot
                    .panes
                    .iter()
                    .find(|pane| pane.pane_id == id)
                    .map(|pane| pane.workspace_id.as_str()),
            }
        });
        requested.map_or(workspace.focused, |id| workspace.workspace_id == id)
    }
}

impl<T: AsRef<str>> NavigationTarget<T> {
    /// Borrow the payload, as `Option::as_deref` does. Not `AsRef`, which
    /// returns a reference to the same value rather than a new target.
    pub(crate) fn as_deref(&self) -> NavigationTarget<&str> {
        match self {
            Self::Workspace(id) => NavigationTarget::Workspace(id.as_ref()),
            Self::Tab(id) => NavigationTarget::Tab(id.as_ref()),
            Self::Pane(id) => NavigationTarget::Pane(id.as_ref()),
        }
    }
}

/// Take ownership of a borrowed target. `ToOwned` is not usable here: its
/// blanket `Clone` impl already covers this type and would hand back a
/// borrowing target instead of an owning one.
impl<T: AsRef<str>> From<&NavigationTarget<T>> for OwnedNavigationTarget {
    fn from(target: &NavigationTarget<T>) -> Self {
        match target.as_deref() {
            NavigationTarget::Workspace(id) => Self::Workspace(id.to_owned()),
            NavigationTarget::Tab(id) => Self::Tab(id.to_owned()),
            NavigationTarget::Pane(id) => Self::Pane(id.to_owned()),
        }
    }
}
