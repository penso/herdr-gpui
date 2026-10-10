//! The Devices layout's tree: under each device's header, that device's
//! agents rather than its workspaces, narrowed by a search field above the
//! list. The headers are the spaces list's own, so folding, menus, and the
//! pinned header work as in every other layout.

use crate::{
    HerdrWindow,
    sidebar::{Indicators, agents::sorted_agents, cell::RowContext, layout::SidebarLook, wash},
};
use gpui::{prelude::*, *};

impl HerdrWindow {
    /// What the Devices layout's search field holds.
    pub(super) fn device_tree_query(&self, cx: &App) -> String {
        self.devices_overview
            .tree_search
            .as_ref()
            .map(|search| search.input.read(cx).text().to_owned())
            .unwrap_or_default()
    }

    /// Whether endpoint `index` is listed for `query`: by its name, how it is
    /// reached, or one of its agents or workspaces.
    pub(super) fn device_listed(&self, index: usize, query: &str) -> bool {
        if query.trim().is_empty() {
            return true;
        }
        let Some(endpoint) = self.endpoints.get(index) else {
            return false;
        };
        let live = if index == self.selected_endpoint {
            &self.live
        } else {
            &endpoint.live
        };
        crate::devices_overview::Device::new(index, endpoint, live).matches(query)
    }

    /// The search field over the tree, when the tick has made it.
    pub(super) fn device_tree_search(&self, look: SidebarLook) -> Option<Div> {
        let search = self.devices_overview.tree_search.as_ref()?;
        Some(
            div()
                .debug_selector(|| "device-tree-search".into())
                .flex_none()
                .px(px(look.content_x()))
                .pb(px(4.))
                .child(search.input.clone()),
        )
    }

    /// Appends endpoint `index`'s agents, nested under its header, and
    /// returns the list with how many rows it added and which of them is
    /// the focused agent, for the reveal.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn append_device_agents(
        &self,
        mut list: Stateful<Div>,
        index: usize,
        indicators: Indicators,
        look: SidebarLook,
        width: f32,
        cx: &mut Context<Self>,
    ) -> (Stateful<Div>, usize, Option<usize>) {
        let Some(endpoint) = self.endpoints.get(index) else {
            return (list, 0, None);
        };
        if endpoint.collapsed {
            return (list, 0, None);
        }
        let selected = index == self.selected_endpoint;
        let live = if selected { &self.live } else { &endpoint.live };
        let Some(snapshot) = live
            .snapshot
            .as_deref()
            .filter(|_| live.status.is_connected())
        else {
            return (list, 0, None);
        };
        let theme = &self.theme;
        let row_cx = RowContext {
            indicators,
            font: &self.config.sidebar,
            theme,
            look,
            width,
            nest: look.nest_indent(),
            mark: wash::HostMark::resolve(
                &self.config.sidebar_style,
                &endpoint.label,
                selected,
                theme,
            ),
            // The header above already names the host.
            host: None,
        };
        let mut count = 0;
        let mut focused = None;
        for agent in sorted_agents(snapshot, self.agent_sort) {
            // Configured `[sidebar_layout.agents]` rows apply here as in the
            // agents panel, gap included.
            let Some(lines) = self.configured_agent_lines(agent, snapshot, row_cx.host) else {
                continue;
            };
            if selected && agent.focused {
                focused = Some(count);
            }
            let gap = self.agent_row_gap(count);
            list = list.child(self.agent_cell(index, agent, snapshot, &row_cx, lines, gap, cx));
            count += 1;
        }
        if count == 0 {
            list = list.child(
                div()
                    .pl(px(look.content_x() + look.nest_indent()))
                    .text_color(rgb(theme.muted))
                    .child(crate::sidebar::label_text("no agents")),
            );
            count = 1;
        }
        (list, count, focused)
    }
}
