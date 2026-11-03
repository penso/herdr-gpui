//! Editor groups: a workspace's view split into side-by-side groups, each
//! listing its own tabs and showing one of them.
//!
//! A tab belongs to exactly one group, so it is listed in one strip and live
//! in one place: the daemon sizes a tab for one client, and a native page is
//! placed only once. Each group that shows a terminal has a client connection
//! of its own, and the daemon keeps a focused tab per client, so groups show
//! different Herdr tabs side by side.
use super::TabId;
use serde::{Deserialize, Serialize};

/// Neither group beside a divider may be dragged narrower than this share
/// of the row.
const MIN_SHARE: f32 = 0.08;
/// A saved layout with more groups than this is not restored.
const MAX_SAVED_GROUPS: usize = 32;
/// Nor one whose group names more tabs than this.
const MAX_SAVED_TABS: usize = 512;
const MAX_TAB_ID_BYTES: usize = 256;

/// Names a group within a window. Unique across the window's workspaces, so
/// state keyed by group, such as an address field, is never shared.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct GroupId(u64);

/// Hands out group IDs for one window.
#[derive(Debug, Default)]
pub(crate) struct GroupIds(u64);

impl GroupIds {
    pub(crate) fn next(&mut self) -> GroupId {
        self.0 += 1;
        GroupId(self.0)
    }
}

/// A group and where it sits, which names its debug selectors.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Slot {
    pub(crate) id: GroupId,
    pub(crate) index: usize,
}

impl Slot {
    /// The first group keeps the unsplit names, so a window that never
    /// splits reads as it always has.
    pub(crate) fn selector(self, name: &str) -> String {
        match self.index {
            0 => name.to_owned(),
            index => format!("g{index}-{name}"),
        }
    }
}

/// The tab a group picked.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub(crate) enum Pick {
    Herdr(String),
    Page(TabId),
}

/// What a group draws.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Shown {
    /// The daemon's focused tab.
    Terminal,
    Page(TabId),
    /// A Herdr tab the group's connection does not focus yet.
    Elsewhere(String),
    /// Nothing to show: a group without tabs, or a page that is gone.
    Empty,
}

#[derive(Clone, Debug, PartialEq)]
struct Group {
    id: GroupId,
    /// The tab the group shows, always one of its own. `None` follows the
    /// daemon's focused tab in a lone group, and shows nothing beside
    /// another: following there would move the group whenever the other
    /// focuses a tab of its own.
    pick: Option<Pick>,
    /// The group's share of the row's width.
    share: f32,
}

/// One workspace's groups, left to right.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Layout {
    groups: Vec<Group>,
    active: GroupId,
    /// The group each tab belongs to, in the order the tabs were claimed.
    /// Empty while the layout has one group, which owns every tab. Beside
    /// another, a tab not named here yet belongs to the group in use.
    owners: Vec<(Pick, GroupId)>,
}

/// A layout as saved across restarts: each group's tabs and width, left to
/// right, and the group in use. Group IDs are the window's own and are
/// handed out anew on restore.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct SavedLayout {
    groups: Vec<SavedGroup>,
    active: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct SavedGroup {
    pick: Option<Pick>,
    share: f32,
    /// The tabs the group owns.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tabs: Vec<Pick>,
    /// Read from layouts saved before tabs belonged to one group, when each
    /// group named the tabs it had closed. Never written.
    #[serde(default, skip_serializing)]
    hidden: Vec<Pick>,
}

fn valid_pick(pick: &Pick) -> bool {
    match pick {
        Pick::Herdr(tab) => !tab.is_empty() && tab.len() <= MAX_TAB_ID_BYTES,
        Pick::Page(_) => true,
    }
}

impl SavedLayout {
    /// Whether a saved layout is one the app could have written: bounded,
    /// with widths that are positive and finite, and names of sane length.
    pub(crate) fn valid(&self) -> bool {
        !self.groups.is_empty()
            && self.groups.len() <= MAX_SAVED_GROUPS
            && self.active < self.groups.len()
            && self.groups.iter().all(|group| {
                group.share.is_finite()
                    && group.share > 0.
                    && group.pick.as_ref().is_none_or(valid_pick)
                    && group.tabs.len() <= MAX_SAVED_TABS
                    && group.tabs.iter().all(valid_pick)
                    && group.hidden.len() <= MAX_SAVED_TABS
                    && group.hidden.iter().all(valid_pick)
            })
    }

    /// The group each tab belongs to, by position. A layout saved before
    /// tabs belonged to one group names what each group had closed instead.
    /// Such a tab goes to a group that kept it: one showing it, so no group
    /// loses the tab it shows, then one only listing it, the group in use
    /// first either way. A tab every group had closed goes to the group in
    /// use, where it was still listed.
    fn owners(&self) -> Vec<(Pick, usize)> {
        let mut owners: Vec<(Pick, usize)> = Vec::new();
        let mut claim = |pick: &Pick, group: usize| {
            if !owners.iter().any(|(owned, _)| owned == pick) {
                owners.push((pick.clone(), group));
            }
        };
        for (index, group) in self.groups.iter().enumerate() {
            for pick in &group.tabs {
                claim(pick, index);
            }
        }
        let kept = |pick: &Pick, group: &SavedGroup| !group.hidden.contains(pick);
        let closed = self.groups.iter().flat_map(|group| &group.hidden);
        let shown = self.groups.iter().filter_map(|group| group.pick.as_ref());
        for pick in shown.chain(closed) {
            let shows = |group: &SavedGroup| group.pick.as_ref() == Some(pick) && kept(pick, group);
            let keeps = |group: &SavedGroup| kept(pick, group);
            let active = self.groups.get(self.active);
            let using = |wanted: &dyn Fn(&SavedGroup) -> bool| {
                Some(self.active).filter(|_| active.is_some_and(wanted))
            };
            let owner = using(&shows)
                .or_else(|| self.groups.iter().position(shows))
                .or_else(|| using(&keeps))
                .or_else(|| self.groups.iter().position(keeps))
                .unwrap_or(self.active);
            claim(pick, owner);
        }
        owners
    }
}

impl Layout {
    /// The layout to save, or `None` for one group following the terminal,
    /// which is what a workspace shows without one.
    pub(crate) fn saved(&self) -> Option<SavedLayout> {
        if self.groups.len() == 1 && self.groups[0].pick.is_none() {
            return None;
        }
        Some(SavedLayout {
            groups: self
                .groups
                .iter()
                .map(|group| SavedGroup {
                    pick: group.pick.clone(),
                    share: group.share,
                    tabs: self
                        .owners
                        .iter()
                        .filter(|(_, owner)| *owner == group.id)
                        .map(|(pick, _)| pick.clone())
                        .collect(),
                    hidden: Vec::new(),
                })
                .collect(),
            active: self
                .groups
                .iter()
                .position(|group| group.id == self.active)
                .unwrap_or(0),
        })
    }

    /// A layout rebuilt from `saved`, with IDs from `ids`. Widths are
    /// scaled to fill the row. A group saved showing a tab that is not its
    /// own shows nothing until it is given one.
    pub(crate) fn restore(saved: &SavedLayout, ids: &mut GroupIds) -> Self {
        let total: f32 = saved.groups.iter().map(|group| group.share).sum();
        let mut groups: Vec<Group> = saved
            .groups
            .iter()
            .map(|group| Group {
                id: ids.next(),
                pick: group.pick.clone(),
                share: group.share / total,
            })
            .collect();
        let active = groups[saved.active.min(groups.len() - 1)].id;
        let owners: Vec<(Pick, GroupId)> = if groups.len() > 1 {
            saved
                .owners()
                .into_iter()
                .map(|(pick, index)| (pick, groups[index].id))
                .collect()
        } else {
            Vec::new()
        };
        for group in &mut groups {
            let owned = |pick: &Pick| {
                owners
                    .iter()
                    .any(|(tab, owner)| tab == pick && *owner == group.id)
            };
            if owners.is_empty() || group.pick.as_ref().is_some_and(owned) {
                continue;
            }
            group.pick = None;
        }
        Self {
            groups,
            active,
            owners,
        }
    }

    /// One group following the terminal, as an unsplit window shows.
    pub(crate) fn new(id: GroupId) -> Self {
        Self {
            groups: vec![Group {
                id,
                pick: None,
                share: 1.,
            }],
            active: id,
            owners: Vec::new(),
        }
    }

    pub(crate) fn slots(&self) -> impl Iterator<Item = Slot> + '_ {
        self.groups.iter().enumerate().map(|(index, group)| Slot {
            id: group.id,
            index,
        })
    }

    pub(crate) fn len(&self) -> usize {
        self.groups.len()
    }

    pub(crate) fn active(&self) -> GroupId {
        self.active
    }

    fn group(&self, id: GroupId) -> Option<&Group> {
        self.groups.iter().find(|group| group.id == id)
    }

    fn group_mut(&mut self, id: GroupId) -> Option<&mut Group> {
        self.groups.iter_mut().find(|group| group.id == id)
    }

    pub(crate) fn share(&self, id: GroupId) -> f32 {
        self.group(id).map_or(0., |group| group.share)
    }

    /// The tab `id` shows, reading a lone group that follows the terminal
    /// as the daemon's `focused` tab.
    pub(crate) fn pick(&self, id: GroupId, focused: Option<&str>) -> Option<Pick> {
        let group = self.group(id)?;
        group.pick.clone().or_else(|| {
            focused
                .filter(|_| self.groups.len() == 1)
                .map(|tab| Pick::Herdr(tab.to_owned()))
        })
    }

    /// What `id` draws, where `focused` names the tab each group's own
    /// connection focuses.
    pub(crate) fn shown(&self, id: GroupId, focused: impl Fn(GroupId) -> Option<String>) -> Shown {
        let focused = focused(id);
        match self.pick(id, focused.as_deref()) {
            Some(Pick::Page(page)) => Shown::Page(page),
            Some(Pick::Herdr(tab)) if focused.as_deref() == Some(tab.as_str()) => Shown::Terminal,
            Some(Pick::Herdr(tab)) => Shown::Elsewhere(tab),
            // Following a terminal the daemon has not focused: the terminal
            // area still shows, as it does before a workspace connects.
            None if self.groups.len() == 1 => Shown::Terminal,
            None => Shown::Empty,
        }
    }

    /// The Herdr tab each group shows: the groups that need a terminal
    /// connection. A group following the terminal is left out; it shows
    /// whatever the window's own connection focuses.
    pub(crate) fn terminal_holders(&self) -> Vec<(GroupId, String)> {
        self.groups
            .iter()
            .filter_map(|group| match &group.pick {
                Some(Pick::Herdr(tab)) => Some((group.id, tab.clone())),
                _ => None,
            })
            .collect()
    }

    /// Makes `id` the group in use. Returns whether anything changed.
    pub(crate) fn activate(&mut self, id: GroupId) -> bool {
        if self.active == id || self.group(id).is_none() {
            return false;
        }
        self.settle_owners();
        self.active = id;
        true
    }

    /// Names the group in use as the owner of each tab in `tabs` that has
    /// none yet. A lone group owns every tab without being named.
    pub(crate) fn adopt(&mut self, tabs: impl IntoIterator<Item = Pick>) {
        if self.groups.len() < 2 {
            return;
        }
        for pick in tabs {
            if self.claimed(&pick).is_none() {
                self.owners.push((pick, self.active));
            }
        }
    }

    /// Before the group in use changes, the tabs it showed without being
    /// named their owner stay its own.
    fn settle_owners(&mut self) {
        let shown: Vec<Pick> = self
            .groups
            .iter()
            .filter_map(|group| group.pick.clone())
            .collect();
        for pick in shown {
            if self.claimed(&pick).is_none()
                && let Some(group) = self.groups.iter().find(|g| g.pick.as_ref() == Some(&pick))
            {
                self.owners.push((pick, group.id));
            }
        }
    }

    /// The group named as `pick`'s owner, if any.
    pub(crate) fn claimed(&self, pick: &Pick) -> Option<GroupId> {
        self.owners
            .iter()
            .find(|(tab, _)| tab == pick)
            .map(|(_, group)| *group)
    }

    /// The group `pick` belongs to: the one named its owner, else the group
    /// in use, which is every tab's owner while it is the only group.
    pub(crate) fn owner(&self, pick: &Pick) -> GroupId {
        self.claimed(pick)
            .filter(|_| self.groups.len() > 1)
            .unwrap_or(self.active)
    }

    /// Whether `id`'s strip lists `pick`: a tab is listed in the one group
    /// it belongs to.
    pub(crate) fn lists(&self, id: GroupId, pick: &Pick) -> bool {
        self.group(id).is_some() && self.owner(pick) == id
    }

    /// Gives `pick` to `id`, taking it from the group that had it, which
    /// shows nothing until it is given another of its tabs.
    fn claim(&mut self, id: GroupId, pick: &Pick) {
        for group in &mut self.groups {
            if group.id != id && group.pick.as_ref() == Some(pick) {
                group.pick = None;
            }
        }
        if self.groups.len() < 2 {
            return;
        }
        match self.owners.iter_mut().find(|(tab, _)| tab == pick) {
            Some(owned) => owned.1 = id,
            None => self.owners.push((pick.clone(), id)),
        }
    }

    /// Lists `pick` in `id`'s strip without showing it, so what the group
    /// shows stays as it is. The tab becomes the group's own. Only control
    /// requests place tabs this way.
    pub(crate) fn give(&mut self, id: GroupId, pick: &Pick) {
        if self.group(id).is_some() {
            self.claim(id, pick);
        }
    }

    /// Shows `pick` in `id` without using the group. The tab becomes the
    /// group's own.
    pub(crate) fn show(&mut self, id: GroupId, pick: Pick) {
        if self.group(id).is_none() {
            return;
        }
        self.claim(id, &pick);
        if let Some(group) = self.group_mut(id) {
            group.pick = Some(pick);
        }
    }

    /// Shows `pick` in `id` and uses the group. The tab becomes the group's
    /// own.
    pub(crate) fn choose(&mut self, id: GroupId, pick: Pick) {
        if self.group(id).is_some() {
            self.activate(id);
            self.show(id, pick);
        }
    }

    /// Forgets a tab that is gone for good. Returns the group that showed
    /// it, which now shows nothing, or follows the terminal when alone.
    pub(crate) fn forget(&mut self, pick: &Pick) -> Option<GroupId> {
        self.owners.retain(|(tab, _)| tab != pick);
        let group = self
            .groups
            .iter_mut()
            .find(|group| group.pick.as_ref() == Some(pick))?;
        group.pick = None;
        Some(group.id)
    }

    /// Splits `id`: a new group to its right, without tabs, takes half its
    /// width and becomes the group in use. A group that followed the
    /// terminal keeps `focused`, and every tab in `tabs` without an owner
    /// stays with the group in use before the split.
    pub(crate) fn split(
        &mut self,
        id: GroupId,
        new: GroupId,
        focused: Option<&str>,
        tabs: impl IntoIterator<Item = Pick>,
    ) -> bool {
        let Some(index) = self.groups.iter().position(|group| group.id == id) else {
            return false;
        };
        // Beside another, a group showing nothing has no tab to keep.
        if let [lone] = &mut self.groups[..]
            && lone.pick.is_none()
        {
            lone.pick = focused.map(|tab| Pick::Herdr(tab.to_owned()));
        }
        let source = &mut self.groups[index];
        source.share /= 2.;
        let group = Group {
            id: new,
            pick: None,
            share: source.share,
        };
        self.groups.insert(index + 1, group);
        self.adopt(tabs);
        self.settle_owners();
        self.active = new;
        true
    }

    /// Where closing `id` would leave it: its place, its share, and the
    /// group its room goes to. `None` for the last group, which never closes.
    pub(crate) fn fold_target(&self, id: GroupId) -> Option<(usize, f32, GroupId)> {
        if self.groups.len() < 2 {
            return None;
        }
        let index = self.groups.iter().position(|group| group.id == id)?;
        let into = if index == 0 { 1 } else { index - 1 };
        Some((index, self.groups[index].share, self.groups[into].id))
    }

    /// Closes `id`, giving its width and its tabs to the group on its left,
    /// or its right when it was first. The last group never closes.
    pub(crate) fn close(&mut self, id: GroupId) -> bool {
        if self.groups.len() < 2 {
            return false;
        }
        let Some(index) = self.groups.iter().position(|group| group.id == id) else {
            return false;
        };
        self.settle_owners();
        let closed = self.groups.remove(index);
        let neighbour = index.saturating_sub(1);
        self.groups[neighbour].share += closed.share;
        let next = self.groups[neighbour].id;
        for owned in &mut self.owners {
            if owned.1 == id {
                owned.1 = next;
            }
        }
        if self.active == id {
            self.active = next;
        }
        // A lone group owns every tab without being named.
        if self.groups.len() == 1 {
            self.owners.clear();
        }
        true
    }

    /// Moves the divider after the `divider`th group to `offset` into a row
    /// `width` wide. Returns whether it moved.
    pub(crate) fn drag(&mut self, divider: usize, offset: f32, width: f32) -> bool {
        if divider + 1 >= self.groups.len() || width.is_nan() || width <= 0. || !offset.is_finite()
        {
            return false;
        }
        let start: f32 = self.groups[..divider].iter().map(|group| group.share).sum();
        let pair = self.groups[divider].share + self.groups[divider + 1].share;
        if pair < 2. * MIN_SHARE {
            return false;
        }
        let left = (offset / width - start).clamp(MIN_SHARE, pair - MIN_SHARE);
        if left == self.groups[divider].share {
            return false;
        }
        self.groups[divider].share = left;
        self.groups[divider + 1].share = pair - left;
        true
    }

    /// Follows the daemon moving the focus of `group`'s connection to `new`,
    /// from a shortcut, an agent, or a closed tab. A group following the
    /// terminal already shows it.
    pub(crate) fn focus_moved(&mut self, group: GroupId, new: &str) {
        if self.group(group).is_some_and(|group| group.pick.is_some()) {
            self.show(group, Pick::Herdr(new.to_owned()));
        }
    }

    /// Every tab the layout names: the ones groups show and the ones they
    /// own.
    pub(crate) fn tabs(&self) -> impl Iterator<Item = &Pick> + '_ {
        self.groups
            .iter()
            .filter_map(|group| group.pick.as_ref())
            .chain(self.owners.iter().map(|(pick, _)| pick))
    }
}

#[cfg(test)]
mod tests;
