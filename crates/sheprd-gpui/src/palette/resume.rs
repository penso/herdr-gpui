//! Reopening the palette as it was last left, as Telescope's resume does:
//! the same query, filters, and selected row.

use super::{AgentFilter, Filter, Identity, Palette};
use crate::SheprdWindow;
use gpui::*;

/// What a closed palette leaves behind for the next resume.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Resume {
    query: String,
    filter: Filter,
    agent_filter: AgentFilter,
    selected: Option<Identity>,
}

impl Palette {
    pub(crate) fn resume(&self) -> Resume {
        Resume {
            query: self.query.clone(),
            filter: self.filter,
            agent_filter: self.agent_filter,
            selected: self.selected_identity(),
        }
    }
}

impl SheprdWindow {
    /// Opens the palette as it was last closed, or fresh if it never was.
    /// The selected row is chosen again once a ranking lists it, so a project
    /// row still loading is found when it arrives.
    pub(crate) fn resume_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(resume) = self.menu.palette_resume.clone() else {
            self.open_palette(Filter::All, window, cx);
            return;
        };
        self.open_palette(resume.filter, window, cx);
        let Some(palette) = &mut self.menu.palette else {
            return;
        };
        palette.agent_filter = resume.agent_filter;
        palette.restore = resume.selected;
        // Set first, so ranking takes the query as the one the row belongs to
        // rather than a new one that supersedes it.
        palette.query.clone_from(&resume.query);
        let search = palette.search.clone();
        search.update(cx, |input, cx| input.set_text_selected(&resume.query, cx));
        self.filter_palette(&resume.query, cx);
    }
}
