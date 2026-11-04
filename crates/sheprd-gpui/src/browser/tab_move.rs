//! Move Tab Left and Right for browser tabs. This client orders them itself,
//! after the workspace's Herdr tabs, so the daemon's `tab.move` cannot.
use super::{Pick, Store, TabId};
use crate::{SheprdWindow, controls::Command, reorder::Beside};
use gpui::Context;

/// Where Move Tab Left or Right puts `page` among `pages`, the browser tabs
/// of its strip in order: beside its neighbour, or past the other end when
/// it is at one, as Herdr wraps a terminal tab. `None` without another page.
fn moved(pages: &[TabId], page: TabId, command: Command) -> Option<Beside<TabId>> {
    let index = pages.iter().position(|&candidate| candidate == page)?;
    let last = pages.len() - 1;
    if last == 0 {
        return None;
    }
    Some(match command {
        Command::MoveTabNext if index == last => Beside::Before(pages[0]),
        Command::MoveTabNext => Beside::After(pages[index + 1]),
        Command::MoveTabPrevious if index == 0 => Beside::After(pages[last]),
        Command::MoveTabPrevious => Beside::Before(pages[index - 1]),
        _ => return None,
    })
}

impl SheprdWindow {
    /// Runs Move Tab Left or Right on the browser tab the group in use
    /// shows, among the browser tabs of its strip, as dragging it does.
    /// Returns false when the group shows a Herdr tab, which the daemon
    /// moves; asking it to move the one behind a page would move a tab the
    /// user cannot see.
    pub(crate) fn move_page_tab(&mut self, command: Command, cx: &mut Context<Self>) -> bool {
        let Some(group) = self.active_group() else {
            return false;
        };
        let Some(Pick::Page(page)) = self.group_pick(group) else {
            return false;
        };
        let pages: Vec<TabId> = self
            .group_tabs(group, cx)
            .into_iter()
            .filter_map(|pick| match pick {
                Pick::Page(id) => Some(id),
                Pick::Herdr(_) => None,
            })
            .collect();
        if let Some(beside) = moved(&pages, page, command)
            && Store::update(cx, |store| store.move_tab(page, beside))
        {
            cx.notify();
        }
        true
    }
}
