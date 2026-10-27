//! Last visited terminal screens, used only as pictures during navigation.
//! No cached surface is ever installed in the connection's live state.

use crate::navigation::NavigationTarget;
use herdr_client::protocol::{
    CellData, ClientShellSnapshot, PaneSurfaceFrame, PaneSurfacePane, PaneSurfaceSplit,
};
use std::{collections::VecDeque, sync::Arc};

const MAX_SCREENS: usize = 16;
const MAX_BYTES: usize = 16 * 1024 * 1024;

struct Screen {
    tab: String,
    workspace: String,
    zoomed: bool,
    frame: Arc<PaneSurfaceFrame>,
    bytes: usize,
}

#[derive(Default)]
pub(super) struct Cache {
    screens: VecDeque<Screen>,
}

impl Cache {
    pub(super) fn remember(
        &mut self,
        snapshot: &ClientShellSnapshot,
        frame: &Arc<PaneSurfaceFrame>,
    ) {
        let Some(tab) = snapshot
            .tabs
            .iter()
            .find(|tab| Some(&tab.tab_id) == snapshot.focused_tab_id.as_ref())
        else {
            return;
        };
        self.screens
            .retain(|screen| screen.frame.boot_id == snapshot.boot_id && screen.tab != tab.tab_id);
        // Image stores can dwarf the cell grid, and popups belong to the
        // current interaction rather than to a tab's reusable picture.
        if frame.popup.is_some()
            || !frame.frame.graphics.is_empty()
            || !frame.graphics.assets.is_empty()
            || !frame.graphics.placements.is_empty()
            || !frame.graphics.retained_assets.is_empty()
        {
            return;
        }
        let bytes = size_of::<Screen>()
            + size_of::<PaneSurfaceFrame>()
            + tab.tab_id.len()
            + tab.workspace_id.len()
            + frame.boot_id.capacity()
            + frame.panes.capacity() * size_of::<PaneSurfacePane>()
            + frame
                .panes
                .iter()
                .map(|pane| pane.pane_id.capacity())
                .sum::<usize>()
            + frame.splits.capacity() * size_of::<PaneSurfaceSplit>()
            + frame
                .splits
                .iter()
                .map(|split| split.path.capacity())
                .sum::<usize>()
            + frame.frame.cells.capacity() * size_of::<CellData>()
            + frame
                .frame
                .cells
                .iter()
                .map(|cell| cell.symbol.capacity())
                .sum::<usize>()
            + frame.frame.hyperlinks.capacity() * size_of::<String>()
            + frame
                .frame
                .hyperlinks
                .iter()
                .map(String::capacity)
                .sum::<usize>();
        if bytes > MAX_BYTES {
            return;
        }
        self.screens.push_front(Screen {
            tab: tab.tab_id.clone(),
            workspace: tab.workspace_id.clone(),
            zoomed: tab.zoomed,
            frame: frame.clone(),
            bytes,
        });
        let mut total = 0;
        self.screens.retain(|screen| {
            total += screen.bytes;
            total <= MAX_BYTES
        });
        self.screens.truncate(MAX_SCREENS);
    }

    pub(super) fn get(
        &self,
        snapshot: &ClientShellSnapshot,
        target: NavigationTarget<&str>,
        size: (u16, u16),
    ) -> Option<Arc<PaneSurfaceFrame>> {
        let tab = match target {
            NavigationTarget::Workspace(id) => snapshot
                .workspaces
                .iter()
                .find(|workspace| workspace.workspace_id == id)?
                .active_tab_id
                .as_str(),
            NavigationTarget::Tab(id) => id,
            NavigationTarget::Pane(id) => snapshot
                .panes
                .iter()
                .find(|pane| pane.pane_id == id)?
                .tab_id
                .as_str(),
        };
        let current = snapshot.tabs.iter().find(|current| current.tab_id == tab)?;
        let screen = self.screens.iter().find(|screen| screen.tab == tab)?;
        let frame = &screen.frame;
        if frame.boot_id != snapshot.boot_id
            || (frame.frame.width, frame.frame.height) != size
            || screen.workspace != current.workspace_id
            || screen.zoomed != current.zoomed
            || !frame.panes.iter().all(|shown| {
                snapshot
                    .panes
                    .iter()
                    .any(|pane| pane.pane_id == shown.pane_id && pane.tab_id == tab)
            })
            || (!screen.zoomed
                && snapshot
                    .panes
                    .iter()
                    .filter(|pane| pane.tab_id == tab)
                    .count()
                    != frame.panes.len())
            || matches!(target, NavigationTarget::Pane(id) if !frame.panes.iter().any(|pane| pane.pane_id == id))
        {
            return None;
        }
        Some(frame.clone())
    }
}

#[cfg(test)]
mod tests;
