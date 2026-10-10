//! Where a workspace's VS Code tab shows: in the editor groups, listed in
//! the strips and shown as any page is, while staying the workspace's one
//! VS Code tab: it keeps its page, its server check, and its token, and no
//! agent ever reuses it.
use super::{TabId, groups::Shown};
use crate::HerdrWindow;
use gpui::App;

impl HerdrWindow {
    /// Whether some group of the focused workspace shows the page `id`.
    pub(super) fn group_shows_page(&self, id: TabId, cx: &App) -> bool {
        self.group_slots()
            .into_iter()
            .any(|slot| self.group_shown(slot.id, cx) == Shown::Page(id))
    }
}
