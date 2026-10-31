//! The window's editor groups: which group shows which tab, using and
//! splitting groups, and following the daemon's focus. The rules for what a
//! group draws live in [`super::groups`]; this applies them to the focused
//! workspace and carries out what they imply, such as focusing the Herdr tab
//! a group asks for.
use super::tab_appear::{Leaving, Listed};
use super::tab_scroll::Thumb;
use super::{
    Location, Scope, Tab, TabId,
    groups::{GroupId, Layout, Pick, Shown, Slot},
    view::store,
};
use crate::{HerdrWindow, NavigationTarget, controls::Command, search_input::SearchInput};
use gpui::{prelude::*, *};
use std::{collections::hash_map::Entry, time::Instant};

/// The tab a strip listing `tabs` goes to when `pick` leaves it: the nearest
/// one after it that `near` accepts, or else before it.
fn neighbour(tabs: &[Pick], pick: &Pick, near: impl Fn(&Pick) -> bool) -> Option<Pick> {
    let position = tabs.iter().position(|tab| tab == pick)?;
    tabs[position + 1..]
        .iter()
        .find(|tab| near(tab))
        .or_else(|| tabs[..position].iter().rev().find(|tab| near(tab)))
        .cloned()
}

/// The tab Next Tab, Previous Tab, or a tab by number lands on among `tabs`
/// from `current`. Next and Previous wrap around, and a number counts from
/// one, as Herdr's `switch_tab` does.
fn stepped<'a>(tabs: &'a [Pick], current: Option<&Pick>, command: Command) -> Option<&'a Pick> {
    let index = match command {
        Command::TabNumber(number) => usize::from(number).checked_sub(1)?,
        Command::NextTab | Command::PreviousTab => {
            let index = tabs.iter().position(|tab| Some(tab) == current)?;
            let step = if command == Command::NextTab {
                1
            } else {
                tabs.len() - 1
            };
            (index + step) % tabs.len()
        }
        _ => return None,
    };
    tabs.get(index)
}

impl HerdrWindow {
    /// The daemon's focused tab, when it belongs to the focused workspace.
    pub(crate) fn focused_herdr_tab(&self) -> Option<&str> {
        let snapshot = self.live.snapshot.as_ref()?;
        let tab = snapshot.focused_tab_id.as_deref()?;
        snapshot
            .tabs
            .iter()
            .any(|candidate| {
                candidate.tab_id == tab
                    && Some(&candidate.workspace_id) == snapshot.focused_workspace_id.as_ref()
            })
            .then_some(tab)
    }

    fn layout(&self) -> Option<&Layout> {
        self.browser.layouts.get(&self.browser_key()?)
    }

    /// `key`'s groups, made on first use from the layout saved for it, if
    /// any, or else as one group following the terminal.
    fn layout_for(&mut self, key: (Scope, String)) -> &mut Layout {
        let browser = &mut self.browser;
        match browser.layouts.entry(key) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                let layout = match browser.saved.remove(entry.key()) {
                    Some(saved) => {
                        browser.restored = Some(entry.key().clone());
                        Layout::restore(&saved, &mut browser.group_ids)
                    }
                    None => Layout::new(browser.group_ids.next()),
                };
                entry.insert(layout)
            }
        }
    }

    /// Saves each of this window's layouts that changed, for the next start.
    pub(crate) fn save_group_layouts(&mut self, cx: &mut Context<Self>) {
        for (key, layout) in &self.browser.layouts {
            super::Layouts::record(cx, key, layout.saved());
        }
    }

    /// The focused workspace's groups, creating its first one. Render calls
    /// this before drawing so every group it draws has an ID.
    pub(crate) fn ensure_layout(&mut self) -> Option<&mut Layout> {
        let key = self.browser_key()?;
        Some(self.layout_for(key))
    }

    /// The focused workspace's groups, left to right. Without a workspace
    /// the window still draws one group, for the terminal area.
    pub(crate) fn group_slots(&self) -> Vec<Slot> {
        match self.layout() {
            Some(layout) => layout.slots().collect(),
            None => vec![Slot {
                id: self.browser.fallback_group,
                index: 0,
            }],
        }
    }

    pub(crate) fn is_split(&self) -> bool {
        self.layout().is_some_and(|layout| layout.len() > 1)
    }

    pub(crate) fn active_group(&self) -> Option<GroupId> {
        self.layout().map(Layout::active)
    }

    /// The share of the row `group` takes as drawn now: its settled share,
    /// bent while a group beside it opens or folds.
    pub(crate) fn group_share(&self, group: GroupId) -> f32 {
        let Some(layout) = self.layout() else {
            return 1.;
        };
        self.browser.group_motion.share(
            group,
            layout.share(group),
            |other| layout.share(other),
            crate::motion::now(),
        )
    }

    /// How far `group` has opened from its split, or `None` once open.
    pub(crate) fn group_opened(&self, group: GroupId) -> Option<f32> {
        self.browser
            .group_motion
            .opened(group, crate::motion::now())
    }

    /// The closed groups still folding away.
    pub(crate) fn folding_groups(&self) -> Vec<super::Fold> {
        self.browser.group_motion.folding(crate::motion::now())
    }

    /// Whether a group is still opening or folding, so the window draws
    /// another frame.
    pub(crate) fn groups_moving(&mut self) -> bool {
        self.browser.group_motion.animating(crate::motion::now())
    }

    /// The tab `group` picked, the focused one when it follows the terminal.
    pub(crate) fn group_pick(&self, group: GroupId) -> Option<Pick> {
        self.layout()?.pick(group, self.focused_herdr_tab())
    }

    /// Records what `group`'s strip draws; see
    /// [`super::tab_appear::TabAppear::observe`].
    pub(crate) fn observe_strip(&mut self, group: GroupId, tabs: Vec<Listed>, now: Instant) {
        self.browser.appear.observe(group, tabs, now);
    }

    /// The tabs shrinking out of `group`'s strip.
    pub(crate) fn leaving_tabs(&self, group: GroupId) -> Vec<Leaving> {
        self.browser.appear.leaving(group).to_vec()
    }

    /// Forgets the strips of groups that closed, so their tabs never shrink
    /// out of a strip no longer drawn.
    pub(crate) fn forget_gone_strips(&mut self) {
        let layouts = &self.browser.layouts;
        let live = |group| {
            layouts
                .values()
                .any(|layout| layout.slots().any(|slot| slot.id == group))
        };
        self.browser.appear.retain(live);
        self.browser.tab_body.retain(live);
        self.browser
            .group_bounds
            .borrow_mut()
            .retain(|group, _| live(*group));
        self.browser.emptied.retain(|group| live(*group));
        // The group drawn before any workspace has a layout keeps its strip.
        let fallback = self.browser.fallback_group;
        self.browser
            .tab_scroll
            .retain(|group| group == fallback || live(group));
    }

    /// What tracks `group`'s strip's sideways scroll.
    pub(crate) fn strip_scroll(&mut self, group: GroupId) -> ScrollHandle {
        self.browser.tab_scroll.handle(group)
    }

    /// Brings `group`'s chosen tab, at `index` in its strip, into view when
    /// the choice changes or while the tab grows in. Whether the strip needs
    /// another frame to do it.
    pub(crate) fn reveal_tab(
        &mut self,
        group: GroupId,
        pick: &Pick,
        index: usize,
        width: f32,
    ) -> bool {
        let growing = self
            .browser
            .appear
            .growth(group, pick, crate::motion::now())
            .is_some();
        self.browser
            .tab_scroll
            .reveal(group, pick, index, width, growing)
    }

    /// The thumb `group`'s strip draws, when its tabs outgrow it.
    pub(crate) fn strip_thumb(&self, group: GroupId) -> Option<Thumb> {
        self.browser.tab_scroll.thumb(group)
    }

    /// Takes hold of `group`'s strip thumb with the pointer at `x`.
    pub(crate) fn grab_strip_thumb(&mut self, group: GroupId, x: Pixels) {
        self.browser.tab_scroll.grab(group, x);
    }

    /// Drags `group`'s strip thumb to the pointer at `x`; whether it moved.
    pub(crate) fn drag_strip_thumb(&mut self, group: GroupId, x: Pixels) -> bool {
        self.browser.tab_scroll.drag(group, x)
    }

    /// The focused workspace's browser tabs and the labels their strips
    /// show, in strip order.
    pub(crate) fn browser_tab_labels(&self, cx: &App) -> Vec<(Pick, SharedString)> {
        let (Some((scope, workspace)), Some(store)) = (self.browser_key(), store(cx)) else {
            return Vec::new();
        };
        store
            .in_workspace(&scope, &workspace)
            .map(|tab| (Pick::Page(tab.id), super::view::tab_label(&tab.title)))
            .collect()
    }

    /// Whether a tab is still growing or shrinking, so the window draws
    /// another frame.
    pub(crate) fn tabs_growing(&self) -> bool {
        self.browser.appear.animating()
    }

    /// Makes groups open and fold, held where their motion starts.
    #[cfg(test)]
    pub(crate) fn hold_group_motion(&mut self) {
        self.browser.group_motion.enable();
        self.browser.group_motion.freeze(std::time::Duration::ZERO);
    }

    /// Ends every tab's growing and shrinking at once, as a moment later.
    #[cfg(test)]
    pub(crate) fn settle_tabs(&mut self) {
        self.browser.appear = Default::default();
    }

    /// Narrows and fades `tab` while it grows into the strip; a whole tab
    /// is left as it is.
    pub(crate) fn grow_tab<E: Styled>(&self, tab: E, group: GroupId, pick: &Pick) -> E {
        /// Wider than any label a strip shows, so the last frame of growth
        /// does not visibly clamp a long one.
        const GROWN: f32 = 480.;
        match self
            .browser
            .appear
            .growth(group, pick, crate::motion::now())
        {
            Some(k) => tab
                .min_w(px(self.tab_layout().min_width() * k))
                .max_w(px(GROWN * k))
                .overflow_hidden()
                .opacity(k),
            None => tab,
        }
    }

    /// Whether `group`'s strip lists `pick`; see [`Layout::lists`]. A tab
    /// this window hides is listed only while the group shows it.
    pub(crate) fn group_lists(&self, group: GroupId, pick: &Pick) -> bool {
        if self.hides_tab(pick) && self.shown_pick(group).as_ref() != Some(pick) {
            return false;
        }
        let Some(layout) = self.layout() else {
            return true;
        };
        layout.lists(group, pick)
    }

    /// What `group` shows, reading a window without groups yet as one
    /// following the terminal.
    fn shown_pick(&self, group: GroupId) -> Option<Pick> {
        let focused = self.focused_herdr_tab();
        match self.layout() {
            Some(layout) => layout.pick(group, focused),
            None => focused.map(|tab| Pick::Herdr(tab.to_owned())),
        }
    }

    /// Whether this window keeps `pick` out of the focused workspace's
    /// strips; see [`super::hidden`].
    fn hides_tab(&self, pick: &Pick) -> bool {
        !self.browser.hidden.is_empty()
            && self
                .browser_key()
                .is_some_and(|key| self.browser.hidden.hides(&key, pick))
    }

    /// The focused workspace's tabs this window lists, in strip order:
    /// Herdr tabs first, then browser tabs. A hidden tab counts while a
    /// group shows it.
    fn workspace_tabs(&self, cx: &App) -> Vec<Pick> {
        let slots = self.group_slots();
        self.open_workspace_tabs(cx)
            .into_iter()
            .filter(|pick| {
                !self.hides_tab(pick)
                    || slots
                        .iter()
                        .any(|slot| self.shown_pick(slot.id).as_ref() == Some(pick))
            })
            .collect()
    }

    /// Every tab of the focused workspace, hidden ones included.
    pub(crate) fn open_workspace_tabs(&self, cx: &App) -> Vec<Pick> {
        let herdr = self.live.snapshot.as_ref().map(|snapshot| {
            snapshot
                .tabs
                .iter()
                .filter(|tab| Some(&tab.workspace_id) == snapshot.focused_workspace_id.as_ref())
                .map(|tab| Pick::Herdr(tab.tab_id.clone()))
                .collect::<Vec<_>>()
        });
        herdr
            .unwrap_or_default()
            .into_iter()
            .chain(self.browser_tab_ids(cx).into_iter().map(Pick::Page))
            .collect()
    }

    /// The tabs `group`'s strip lists, in strip order.
    pub(crate) fn group_tabs(&self, group: GroupId, cx: &App) -> Vec<Pick> {
        self.workspace_tabs(cx)
            .into_iter()
            .filter(|pick| self.group_lists(group, pick))
            .collect()
    }

    /// Moves `pick` to `group` and shows it there, as dropping its tab on
    /// the group does, or asking there for a tab a workspace has one of. The
    /// group it leaves goes to the tab after it in its strip, or before it,
    /// and closes when it was its last.
    pub(crate) fn move_tab_to_group(
        &mut self,
        pick: Pick,
        group: GroupId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(source) = self.ensure_layout().map(|layout| layout.owner(&pick)) else {
            return;
        };
        if !self.group_slots().iter().any(|slot| slot.id == group) {
            return;
        }
        if source == group {
            self.show_group_tab(group, pick, window, cx);
            return;
        }
        let before = self.group_tabs(source, cx);
        let shown = self.group_pick(source).as_ref() == Some(&pick);
        self.show_group_tab(group, pick.clone(), window, cx);
        if !shown {
            return;
        }
        if !self.show_neighbour(source, &before, &pick, cx) {
            self.close_group(source, window, cx);
        }
        cx.notify();
    }

    /// Moves the group showing `pick` on to the tab after it in its strip,
    /// or before it, as when the tab is carried off to another window. A
    /// group with no other tab keeps showing it.
    pub(crate) fn leave_tab(&mut self, pick: &Pick, cx: &mut Context<Self>) {
        let Some(source) = self.ensure_layout().map(|layout| layout.owner(pick)) else {
            return;
        };
        if self.group_pick(source).as_ref() != Some(pick) {
            return;
        }
        let before = self.group_tabs(source, cx);
        if self.show_neighbour(source, &before, pick, cx) {
            cx.notify();
        }
    }

    /// Shows in `source` the neighbour of `pick` among `before`, its strip
    /// as it was; whether it had one.
    fn show_neighbour(
        &mut self,
        source: GroupId,
        before: &[Pick],
        pick: &Pick,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(next) = neighbour(before, pick, |_| true) else {
            return false;
        };
        if let Some(layout) = self.ensure_layout() {
            layout.show(source, next);
        }
        self.reconcile_group_terminals(cx);
        self.steer_primary_group(source, cx);
        true
    }

    /// Runs Next Tab, Previous Tab, or a tab by number over the focused
    /// workspace's tabs, its browser tabs included, from the tab the group in
    /// use shows. The daemon knows only its own tabs, so asking it alone
    /// would skip the pages, and would leave a page in front of the Herdr tab
    /// it already focuses. From a terminal to a terminal, the daemon still
    /// moves its focus and the groups follow; a step from or to a page shows
    /// its tab in its own group, as clicking it does. Returns false without a
    /// focused workspace.
    pub(crate) fn step_tab(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(active) = self.ensure_layout().map(|layout| layout.active()) else {
            return false;
        };
        // A focus change in flight lands after this step and would move the
        // group off a page the step showed, so the step is dropped, as one
        // asked of the daemon always was.
        if self.activation_deadline.is_some() {
            return true;
        }
        let tabs = self.workspace_tabs(cx);
        let current = self.group_pick(active).or_else(|| {
            self.focused_herdr_tab()
                .map(|tab| Pick::Herdr(tab.to_owned()))
        });
        let Some(target) = stepped(&tabs, current.as_ref(), command).cloned() else {
            return true;
        };
        match target {
            Pick::Herdr(tab) if !matches!(current, Some(Pick::Page(_))) => {
                self.navigate(NavigationTarget::Tab(&tab), cx);
                window.focus(&self.focus, cx);
            }
            target => {
                if let Some(group) = self.layout().map(|layout| layout.owner(&target)) {
                    self.show_group_tab(group, target, window, cx);
                }
            }
        }
        true
    }

    /// Shows `pick` in `group`, which owns it.
    fn show_group_tab(
        &mut self,
        group: GroupId,
        pick: Pick,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match pick {
            Pick::Herdr(tab) => self.choose_herdr_tab(group, &tab, window, cx),
            Pick::Page(id) => self.show_browser_tab_in(Some(group), id, window, cx),
        }
    }

    /// Steers a group's existing primary connection after replacing its tab,
    /// without activating the group or taking keyboard focus from a page.
    fn steer_primary_group(&mut self, group: GroupId, cx: &mut Context<Self>) {
        if Some(group) != self.primary_group() {
            return;
        }
        if let Some(Pick::Herdr(tab)) = self.group_pick(group)
            && self.focused_herdr_tab() != Some(tab.as_str())
        {
            if self.input_ready() && self.menu.page.is_none() {
                self.navigate(NavigationTarget::Tab(&tab), cx);
            } else {
                self.pending_navigation = Some(NavigationTarget::Tab(tab));
            }
        }
    }

    /// What `group` draws. A page another window closed reads as nothing
    /// until the next tick forgets it.
    pub(crate) fn group_shown(&self, group: GroupId, cx: &App) -> Shown {
        let Some(layout) = self.layout() else {
            return Shown::Terminal;
        };
        let focused = |group| self.group_focused_tab(group);
        let shown = layout.shown(group, focused);
        match &shown {
            Shown::Page(id) if store(cx).is_none_or(|store| store.get(*id).is_none()) => {
                Shown::Empty
            }
            // The window's connection is on its way to the tab its group
            // picked: the terminal keeps its frame until the daemon focuses
            // the tab, rather than flashing a stand-in for one round trip.
            Shown::Elsewhere(tab)
                if Some(group) == self.primary_group() && !self.parked_focuses(tab) =>
            {
                Shown::Terminal
            }
            _ => shown,
        }
    }

    /// The group `pick` is live in, if any.
    #[cfg(any(target_os = "macos", windows))]
    pub(crate) fn group_showing(&self, pick: &Pick, cx: &App) -> Option<GroupId> {
        self.group_slots()
            .into_iter()
            .map(|slot| slot.id)
            .find(|group| match (self.group_shown(*group, cx), pick) {
                (Shown::Terminal, Pick::Herdr(_)) => self.group_pick(*group).as_ref() == Some(pick),
                (Shown::Page(id), Pick::Page(page)) => id == *page,
                _ => false,
            })
    }

    /// The pages live in some group, which the window presents.
    #[cfg(any(target_os = "macos", windows))]
    pub(crate) fn live_pages(&self, cx: &App) -> Vec<TabId> {
        self.drawn_group_slots()
            .into_iter()
            .filter_map(|slot| match self.group_shown(slot.id, cx) {
                Shown::Page(id) => Some(id),
                _ => None,
            })
            .collect()
    }

    /// Settles which group holds the window's connection, then, when that
    /// is `group`, focuses the Herdr tab it picked if the connection shows
    /// another, and hands it the keyboard. A group with a parked connection
    /// steers that one to its tab instead.
    fn bring_group_tab(&mut self, group: GroupId, window: &mut Window, cx: &mut Context<Self>) {
        self.reconcile_group_terminals(cx);
        if self.primary_group() != Some(group) {
            return;
        }
        // This choice supersedes any deferred target, including when its
        // terminal is already focused and needs no new request.
        self.pending_navigation = None;
        self.steer_primary_group(group, cx);
        if matches!(self.group_pick(group), Some(Pick::Herdr(_)) | None) {
            window.focus(&self.focus, cx);
        }
    }

    /// Makes `group` the one in use, which brings it the tab it picked.
    pub(crate) fn activate_group(
        &mut self,
        group: GroupId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(layout) = self.ensure_layout() else {
            return;
        };
        if !layout.activate(group) {
            return;
        }
        self.bring_group_tab(group, window, cx);
        cx.notify();
    }

    /// Shows Herdr tab `tab` in `group`.
    pub(crate) fn choose_herdr_tab(
        &mut self,
        group: GroupId,
        tab: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(layout) = self.ensure_layout() else {
            return;
        };
        layout.choose(group, Pick::Herdr(tab.to_owned()));
        self.bring_group_tab(group, window, cx);
        cx.notify();
    }

    /// The next Herdr tab the daemon focuses goes to `group`: its "+" asked
    /// for it.
    pub(crate) fn expect_new_tab_in(&mut self, group: GroupId) {
        self.browser.new_tab_group = self.browser_key().map(|key| (key, group));
    }

    #[cfg(test)]
    pub(crate) fn expected_new_tab_group(&self) -> Option<GroupId> {
        self.browser.new_tab_group.as_ref().map(|(_, group)| *group)
    }

    /// Follows the window's connection moving its focus, from a shortcut, an
    /// agent, a closed tab, or a group's "+". The tab's own group shows it
    /// and takes the connection; a new tab goes to the group whose "+" asked
    /// for it, or else to the group in use. The group that had the
    /// connection gets a parked one for its own tab. Arriving in another
    /// workspace moves its group only off a Herdr tab: one showing a page
    /// keeps it.
    pub(crate) fn terminal_focus_moved(
        &mut self,
        new: &str,
        arrived: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = self.browser_key() else {
            return;
        };
        // Restore a saved layout before reading its groups.
        self.layout_for(key.clone());
        let asked = self
            .browser
            .new_tab_group
            .take_if(|(asked, _)| *asked == key)
            .map(|(_, group)| group);
        let primary = self.primary_group();
        let on_page =
            primary.is_some_and(|group| matches!(self.group_pick(group), Some(Pick::Page(_))));
        // A restored layout keeps its tabs; the group in use is taken back
        // to its own instead.
        let restoring = self.browser.restored.as_ref() == Some(&key);
        if arrived && (on_page || restoring) {
            return;
        }
        // The daemon moved on from a tab it closed. When that lands in
        // another group, the group that showed the closed tab goes to one of
        // its own instead; see `settle_groups`.
        let left_closed = primary
            .and_then(|group| self.group_pick(group))
            .is_some_and(|pick| !self.workspace_tabs(cx).contains(&pick));
        let pick = Pick::Herdr(new.to_owned());
        let layout = self.layout_for(key);
        let split = layout.len() > 1;
        let target = asked.or_else(|| {
            if !split {
                return None;
            }
            Some(layout.claimed(&pick).unwrap_or(layout.active()))
        });
        // The group holding the connection keeps it for a tab of its own,
        // without becoming the group in use: an agent must not take that
        // from a group showing a tab. A group in use that shows none has
        // nothing to keep, and Close Tab there would close the group rather
        // than the tab the focus just moved to.
        let idle = layout.pick(layout.active(), None).is_none();
        let own = target.is_some()
            && target == primary
            && !idle
            && primary.is_some_and(|primary| layout.pick(primary, None).is_some());
        match (target, primary) {
            (Some(group), Some(primary)) if group != primary && left_closed && asked.is_none() => {}
            (Some(group), _) if !own => {
                layout.choose(group, pick);
                self.set_primary_group(group);
            }
            (_, Some(primary)) => layout.focus_moved(primary, new),
            (_, None) => {}
        }
        self.reconcile_group_terminals(cx);
        cx.notify();
    }

    /// Keeps the focused workspace's groups in step with its tabs, on every
    /// tick: forgets the tabs that closed, names the group in use as the
    /// owner of each tab that has none, gives a group whose tab closed
    /// another of its own, and closes a group whose last tab closed.
    pub(crate) fn settle_groups(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(key), Some(snapshot)) = (self.browser_key(), self.live.snapshot.clone()) else {
            return;
        };
        if snapshot.focused_workspace_id.as_ref() != Some(&key.1) {
            return;
        }
        let open = self.open_workspace_tabs(cx);
        self.browser.hidden.retain_open(&key, &open);
        let tabs = self.workspace_tabs(cx);
        let Some(layout) = self.browser.layouts.get_mut(&key) else {
            return;
        };
        let closed: Vec<Pick> = layout
            .tabs()
            .filter(|pick| !tabs.contains(pick))
            .cloned()
            .collect();
        // With the groups whose tab was found closed before this ran.
        let mut emptied: Vec<GroupId> = layout
            .slots()
            .map(|slot| slot.id)
            .filter(|group| self.browser.emptied.contains(group))
            .collect();
        self.browser
            .emptied
            .retain(|group| !emptied.contains(group));
        for pick in &closed {
            emptied.extend(layout.forget(pick));
        }
        layout.adopt(tabs);
        let mut changed = !closed.is_empty();
        if layout.len() > 1 {
            for slot in self.group_slots() {
                if self.group_pick(slot.id).is_some() {
                    continue;
                }
                match self.group_tabs(slot.id, cx).into_iter().next() {
                    Some(next) => {
                        if let Some(layout) = self.browser.layouts.get_mut(&key) {
                            layout.show(slot.id, next);
                        }
                        // The window's connection goes to its group's tab;
                        // a parked one is steered there as it is reconciled.
                        self.steer_primary_group(slot.id, cx);
                        changed = true;
                    }
                    None if emptied.contains(&slot.id) => {
                        self.close_group(slot.id, window, cx);
                        changed = true;
                    }
                    None => {}
                }
            }
        }
        if changed {
            cx.notify();
        }
    }

    /// Shows a browser tab in its group, or in the group in use when it is
    /// new.
    pub(crate) fn show_browser_tab(
        &mut self,
        id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_browser_tab_in(None, id, window, cx);
    }

    /// Shows a browser tab in `group`, which takes it, or else in the group
    /// it belongs to: the group in use of its workspace when it is new.
    pub(crate) fn show_browser_tab_in(
        &mut self,
        group: Option<GroupId>,
        id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = store(cx).and_then(|store| store.get(id)).cloned() else {
            return;
        };
        let layout = self.layout_for((tab.scope.clone(), tab.workspace_id.clone()));
        let group = group.unwrap_or_else(|| layout.owner(&Pick::Page(id)));
        layout.choose(group, Pick::Page(id));
        // A page chosen in the primary group replaces its pending terminal
        // choice. Pages in other groups leave that navigation alone.
        if self.primary_group() == Some(group) {
            self.pending_navigation = None;
        }
        if let Some(editor) = tab.place.editor() {
            self.ensure_editor_page(editor, window, cx);
        } else {
            #[cfg(any(target_os = "macos", windows))]
            match self.browser.pages.ensure(&tab, window, cx) {
                Ok(()) => {
                    self.browser.failed.remove(&id);
                }
                Err(error) => {
                    tracing::warn!(%error, "Cannot create a browser page");
                    self.browser.failed.insert(id, error.to_string().into());
                }
            }
        }
        self.sync_address(group, Some(&tab), true, window, cx);
        if tab.location.is_none() && !tab.place.is_editor() {
            let focus = self.group_address(group, cx).read(cx).focus.clone();
            window.focus(&focus, cx);
        }
        if let Some(Location::CoreTerminal { .. }) = &tab.location {
            self.focus_core_terminal(id, window, cx);
        }
        cx.notify();
    }

    /// Opens a blank browser tab in `group`.
    pub(crate) fn open_browser_tab_in(
        &mut self,
        group: GroupId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(layout) = self.ensure_layout() {
            layout.activate(group);
        }
        self.open_browser_tab(None, window, cx);
    }

    /// Closes a browser tab. The group showing it moves to the tab after it
    /// in its strip, or before it; a group of a split left with none closes.
    pub(crate) fn close_browser_tab(
        &mut self,
        id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = store(cx).and_then(|store| store.get(id)).cloned() else {
            return;
        };
        let pick = Pick::Page(id);
        let key = (tab.scope.clone(), tab.workspace_id.clone());
        // Only the focused workspace's strips are known here; another
        // workspace's group is given a tab when it is next shown.
        let focused = self.browser_key().as_ref() == Some(&key);
        let group = self
            .browser
            .layouts
            .get(&key)
            .filter(|_| focused)
            .map(|layout| layout.owner(&pick));
        let before = group.map_or_else(Vec::new, |group| self.group_tabs(group, cx));
        let shown = self
            .browser
            .layouts
            .get_mut(&key)
            .and_then(|layout| layout.forget(&pick));
        crate::browser::Store::update(cx, |store| store.close(id));
        self.forget_browser_tabs(|tab| tab == id);
        if !focused {
            self.browser.emptied.extend(shown);
        }
        if let (Some(group), Some(shown)) = (group, shown)
            && group == shown
        {
            // Alone, a group moves among its browser tabs and otherwise
            // follows the terminal again, wherever the daemon's focus is.
            let split = self.is_split();
            let near = |tab: &Pick| split || matches!(tab, Pick::Page(_));
            match neighbour(&before, &pick, near) {
                Some(next) => self.show_group_tab(group, next, window, cx),
                None if split => self.close_group(group, window, cx),
                None => {}
            }
        }
        cx.notify();
    }

    /// Splits `group`: a new group opens to its right without tabs, and
    /// every tab stays where it is.
    pub(crate) fn split_group(
        &mut self,
        group: GroupId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let new = self.browser.group_ids.next();
        let focused = self.focused_herdr_tab().map(str::to_owned);
        let tabs = self.workspace_tabs(cx);
        // The group holding the window's connection keeps it: the new group
        // has no tab to show yet.
        let primary = self.primary_group();
        let Some(layout) = self.ensure_layout() else {
            return;
        };
        if !layout.split(group, new, focused.as_deref(), tabs) {
            return;
        }
        if let Some(primary) = primary {
            self.set_primary_group(primary);
        }
        self.browser
            .group_motion
            .open(new, group, crate::motion::now());
        self.bring_group_tab(new, window, cx);
        self.sync_addresses(true, window, cx);
        cx.notify();
    }

    /// Splits the group in use.
    pub(crate) fn split_active_group(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(group) = self.ensure_layout().map(|layout| layout.active()) {
            self.split_group(group, window, cx);
        }
    }

    /// Closes `group`; its tabs go to the group that takes its room.
    pub(crate) fn close_group(
        &mut self,
        group: GroupId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(layout) = self.ensure_layout() else {
            return;
        };
        let target = layout.fold_target(group);
        if !layout.close(group) {
            return;
        }
        let active = layout.active();
        if let Some((index, share, into)) = target {
            self.browser
                .group_motion
                .fold(group, index, share, into, crate::motion::now());
        }
        self.browser.addresses.remove(&group);
        self.forget_group_terminal(group);
        self.bring_group_tab(active, window, cx);
        self.sync_addresses(true, window, cx);
        cx.notify();
    }

    /// Moves the divider after the `divider`th group; see [`Layout::drag`].
    pub(crate) fn drag_divider(&mut self, divider: usize, offset: f32, width: f32) -> bool {
        self.ensure_layout()
            .is_some_and(|layout| layout.drag(divider, offset, width))
    }

    /// `group`'s address field, made the first time it shows a page.
    pub(crate) fn group_address(&mut self, group: GroupId, cx: &mut App) -> Entity<SearchInput> {
        self.browser
            .addresses
            .entry(group)
            .or_insert_with(|| {
                let input = cx.new(|cx| {
                    let mut input = SearchInput::new(cx);
                    input.set_placeholder("Enter an address", cx);
                    input
                });
                (input, None)
            })
            .0
            .clone()
    }

    /// Shows each live page's address in its group's field, unless someone
    /// is typing there.
    pub(crate) fn sync_addresses(&mut self, force: bool, window: &Window, cx: &mut Context<Self>) {
        for slot in self.group_slots() {
            if let Shown::Page(id) = self.group_shown(slot.id, cx) {
                let tab = store(cx).and_then(|store| store.get(id)).cloned();
                self.sync_address(slot.id, tab.as_ref(), force, window, cx);
            }
        }
    }

    pub(crate) fn sync_address(
        &mut self,
        group: GroupId,
        tab: Option<&Tab>,
        force: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let input = self.group_address(group, cx);
        let id = tab.map(|tab| tab.id);
        let text = tab
            .and_then(|tab| tab.location.as_ref())
            .map(Location::display)
            .unwrap_or_default();
        let shown = self.browser.addresses.get(&group).and_then(|(_, tab)| *tab);
        let field = input.read(cx);
        if !force
            && (field.focus.is_focused(window) || (shown == id && field.text() == text.as_str()))
        {
            return;
        }
        if let Some((_, tab)) = self.browser.addresses.get_mut(&group) {
            *tab = id;
        }
        input.update(cx, |input, cx| input.set_text_selected(&text, cx));
    }
}
