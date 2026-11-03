//! Which devices the sidebar lists. The device on screen is always listed:
//! navigation that leaves the scope narrows it to the new device, and a scope
//! is never empty.
use std::collections::BTreeSet;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum DeviceScope {
    #[default]
    All,
    /// A non-empty set of device IDs.
    Only(BTreeSet<String>),
}

impl DeviceScope {
    pub(crate) fn only(id: impl Into<String>) -> Self {
        Self::Only(BTreeSet::from([id.into()]))
    }

    pub(crate) fn shows(&self, id: &str) -> bool {
        match self {
            Self::All => true,
            Self::Only(ids) => ids.contains(id),
        }
    }

    /// Whether `id` is picked by name, which All Devices picks none of.
    pub(crate) fn picks(&self, id: &str) -> bool {
        matches!(self, Self::Only(ids) if ids.contains(id))
    }

    /// The IDs picked by name; empty for All Devices.
    pub(crate) fn picked(&self) -> impl Iterator<Item = &str> {
        match self {
            Self::All => None,
            Self::Only(ids) => Some(ids.iter().map(String::as_str)),
        }
        .into_iter()
        .flatten()
    }

    /// Keeps the newly selected device listed. A scope that already shows it
    /// stays as it is; otherwise the sidebar follows to that device alone.
    pub(crate) fn follow(&mut self, id: &str) {
        if !self.shows(id) {
            *self = Self::only(id);
        }
    }

    /// The scope with `id` added or taken away. All Devices starts from the
    /// `selected` device, so picking another lists the two of them. `None` when
    /// taking it away would list nothing.
    pub(crate) fn toggled<'a>(
        &self,
        id: &str,
        selected: &str,
        enabled: impl IntoIterator<Item = &'a str>,
    ) -> Option<Self> {
        let mut ids = match self {
            Self::All => BTreeSet::from([selected.to_owned(), id.to_owned()]),
            Self::Only(ids) => {
                let mut ids = ids.clone();
                if !ids.remove(id) {
                    ids.insert(id.to_owned());
                }
                ids
            }
        };
        let enabled: BTreeSet<&str> = enabled.into_iter().collect();
        ids.retain(|id| enabled.contains(id.as_str()));
        if ids.is_empty() {
            return None;
        }
        // Naming every device is All Devices, so a device added later is
        // listed too.
        Some(if enabled.iter().all(|id| ids.contains(*id)) {
            Self::All
        } else {
            Self::Only(ids)
        })
    }

    /// Drops devices that are no longer `enabled`, such as ones that left the
    /// catalog. Navigation has already followed a retired selected device, so
    /// the scope only falls back to All Devices if nothing it named is left.
    pub(crate) fn reconcile<'a>(&mut self, enabled: impl IntoIterator<Item = &'a str>) {
        let Self::Only(ids) = self else {
            return;
        };
        let enabled: BTreeSet<&str> = enabled.into_iter().collect();
        ids.retain(|id| enabled.contains(id.as_str()));
        if ids.is_empty() {
            *self = Self::All;
        }
    }
}
