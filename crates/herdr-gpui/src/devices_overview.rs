//! The Devices overview: a tab, beside the workspace's terminals and pages,
//! that shows every device this window connects to, how many agents work on
//! each, the agent activity of the last two hours, and each machine's CPU,
//! memory, home disk and uptime. Being a tab, it can sit in a group of its
//! own or take a window to itself.
//!
//! Everything it shows is already in the window: the endpoints' snapshots,
//! the system load samples, and the activity history the tick keeps. Drawing
//! reads that prepared state only.

mod chart;
pub(crate) mod history;
mod model;
mod table;
mod view;

#[cfg(test)]
mod tests;

pub(crate) use chart::sparkline;
pub(crate) use model::{Device, Link, Tally, online_tally};

use crate::{
    HerdrWindow,
    browser::{Location, Store, TabId},
    search_input::{self, SearchInput},
    window::Flash,
};
use gpui::{prelude::*, *};
use std::collections::HashMap;

/// One open overview tab's own state.
pub(crate) struct Page {
    pub(crate) focus: FocusHandle,
    pub(crate) search: Entity<SearchInput>,
    pub(crate) scroll: ScrollHandle,
    /// Whether "View all devices" opened a lane per device.
    pub(crate) lanes: bool,
    /// The devices whose rows are open on their agents, by endpoint id.
    pub(crate) expanded: std::collections::HashSet<String>,
    _changed: Subscription,
}

/// The search field over the sidebar's Devices layout.
pub(crate) struct TreeSearch {
    pub(crate) input: Entity<SearchInput>,
    _changed: Subscription,
}

/// The overview's state in a window: the activity history, kept whether or
/// not a tab shows it so a newly opened one has a past, each tab's page, and
/// the Devices layout's search while that layout is in use.
#[derive(Default)]
pub(crate) struct Overview {
    pub(crate) history: history::History,
    pub(crate) pages: HashMap<TabId, Page>,
    pub(crate) tree_search: Option<TreeSearch>,
}

impl HerdrWindow {
    /// Opens the Devices overview in a tab of the focused workspace, or
    /// brings back the one already open there.
    pub(crate) fn open_devices_overview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((scope, workspace)) = self.browser_key() else {
            self.show_flash(Flash::warning("Open a workspace first"), cx);
            return;
        };
        let existing = cx.try_global::<Store>().and_then(|store| {
            store
                .in_workspace(&scope, &workspace)
                .find(|tab| matches!(tab.location, Some(Location::Devices)))
                .map(|tab| tab.id)
        });
        let opened = existing.or_else(|| {
            Store::update(cx, |store| {
                store.open(scope, &workspace, Some(Location::Devices), None)
            })
        });
        let Some(id) = opened else {
            self.show_flash(Flash::warning("Too many tabs are open"), cx);
            return;
        };
        self.dismiss_menu(window, cx);
        self.ensure_overview_page(id, cx);
        self.show_browser_tab(id, window, cx);
        if let Some(page) = self.devices_overview.pages.get(&id) {
            window.focus(&page.focus, cx);
        }
        cx.notify();
    }

    fn device_search(
        &self,
        font: crate::config::FontConfig,
        placeholder: &str,
        cx: &mut Context<Self>,
    ) -> (Entity<SearchInput>, Subscription) {
        let theme = self.theme.clone();
        let search = cx.new(|cx| {
            let mut input = SearchInput::new(cx);
            input.set_placeholder(placeholder, cx);
            input.set_appearance(font, theme, cx);
            input
        });
        let changed = cx.subscribe(&search, |_, _, _: &search_input::Changed, cx| cx.notify());
        (search, changed)
    }

    fn ensure_overview_page(&mut self, id: TabId, cx: &mut Context<Self>) {
        if self.devices_overview.pages.contains_key(&id) {
            return;
        }
        let (search, changed) = self.device_search(
            self.config.ui.clone(),
            "Search devices, agents, workspaces",
            cx,
        );
        self.devices_overview.pages.insert(
            id,
            Page {
                focus: cx.focus_handle(),
                search,
                scroll: ScrollHandle::new(),
                lanes: false,
                expanded: Default::default(),
                _changed: changed,
            },
        );
    }

    /// Runs every window tick: records what each device shows for the
    /// activity history, gives restored overview tabs of the focused
    /// workspace a page, and drops the pages of closed tabs.
    pub(crate) fn poll_devices_overview(&mut self, cx: &mut Context<Self>) {
        let now = std::time::Instant::now();
        let wall = std::time::SystemTime::now();
        // Counted straight from each snapshot: this runs every tick, open
        // overview or not, and needs none of a row's display data.
        let selected = self.selected_endpoint;
        let counts = self
            .endpoints
            .iter()
            .enumerate()
            .filter_map(|(index, endpoint)| {
                let live = if index == selected {
                    &self.live
                } else {
                    &endpoint.live
                };
                let tally = online_tally(endpoint, live)?;
                Some((endpoint.id.as_str(), tally.counts()))
            });
        self.devices_overview.history.observe(now, wall, counts);
        self.sync_tree_search(cx);
        let Some(store) = cx.try_global::<Store>() else {
            self.devices_overview.pages.clear();
            return;
        };
        self.devices_overview
            .pages
            .retain(|id, _| store.get(*id).is_some());
        let Some((scope, workspace)) = self.browser_key() else {
            return;
        };
        let missing: Vec<TabId> = store
            .in_workspace(&scope, &workspace)
            .filter(|tab| {
                matches!(tab.location, Some(Location::Devices))
                    && !self.devices_overview.pages.contains_key(&tab.id)
            })
            .map(|tab| tab.id)
            .collect();
        for id in missing {
            self.ensure_overview_page(id, cx);
        }
        // Search fields follow a theme or font change.
        for page in self.devices_overview.pages.values() {
            let (font, theme) = (self.config.ui.clone(), self.theme.clone());
            page.search.update(cx, |input, cx| {
                input.set_appearance(font, theme, cx);
            });
        }
    }

    /// Gives the Devices layout its search field while it is in use, in the
    /// sidebar's face, and drops it, with what it held, otherwise.
    fn sync_tree_search(&mut self, cx: &mut Context<Self>) {
        if self.config.layout.mode != crate::config::LayoutMode::Devices {
            self.devices_overview.tree_search = None;
            return;
        }
        let font = self.config.sidebar.clone();
        match &self.devices_overview.tree_search {
            Some(search) => {
                let theme = self.theme.clone();
                search.input.update(cx, |input, cx| {
                    input.set_appearance(font, theme, cx);
                });
            }
            None => {
                let (input, changed) = self.device_search(font, "Search devices and agents", cx);
                self.devices_overview.tree_search = Some(TreeSearch {
                    input,
                    _changed: changed,
                });
                cx.notify();
            }
        }
    }

    /// Whether an overview tab is open, so every host's load is sampled.
    pub(crate) fn devices_overview_open(&self) -> bool {
        !self.devices_overview.pages.is_empty()
    }

    /// Every device the window knows, in sidebar order, read from the live
    /// state each draws from.
    pub(crate) fn overview_devices(&self) -> Vec<Device> {
        self.endpoints
            .iter()
            .enumerate()
            .map(|(index, endpoint)| {
                let live = if index == self.selected_endpoint {
                    &self.live
                } else {
                    &endpoint.live
                };
                Device::new(index, endpoint, live)
            })
            .collect()
    }
}
