//! What the app keeps on disk, read before the event loop so the first frame
//! does no disk work on the UI thread, and installed once it starts.

use gpui::App;

/// Each store the main view reads. Fixtures and native tests start on the
/// defaults, so they never read, overwrite, or ask about the user's data.
pub(crate) struct Stores {
    agent_skill: crate::agent_skill::AgentSkill,
    browser_tabs: crate::browser::Store,
    dispatch_history: crate::dispatch::History,
    fan_out_journal: crate::fan_out::Journal,
    group_layouts: crate::browser::Layouts,
    #[cfg(windows)]
    jump_list: Option<crate::jump_list::JumpList>,
    keychain_grants: crate::usage::KeychainGrants,
    notification_mutes: crate::notification_mutes::Mutes,
    overview_marks: crate::overview::Marks,
    palette_frecency: crate::palette::Frecency,
    review_viewed: crate::review::Viewed,
    taskbar: crate::taskbar::Taskbar,
    window_state: Option<crate::window_state::WindowState>,
    worktree_landing: crate::worktree_landing::Remembered,
    worktree_notes: crate::worktree_notes::Notes,
}

impl Stores {
    /// Reads the user's stores when `personal`, else takes the defaults.
    pub(crate) fn load(personal: bool) -> Self {
        fn read<T: Default>(personal: bool, load: impl FnOnce() -> T) -> T {
            if personal { load() } else { T::default() }
        }
        Self {
            agent_skill: read(personal, crate::agent_skill::AgentSkill::load),
            browser_tabs: read(personal, crate::browser::Store::load),
            dispatch_history: read(personal, crate::dispatch::History::load),
            fan_out_journal: read(personal, crate::fan_out::Journal::load),
            group_layouts: read(personal, crate::browser::Layouts::load),
            #[cfg(windows)]
            jump_list: personal.then(crate::jump_list::JumpList::load),
            keychain_grants: read(personal, crate::usage::KeychainGrants::load),
            notification_mutes: read(personal, crate::notification_mutes::Mutes::load),
            overview_marks: read(personal, crate::overview::Marks::load),
            palette_frecency: read(personal, crate::palette::Frecency::load),
            review_viewed: read(personal, crate::review::Viewed::load),
            // Whether the launcher is installed is read once here, never
            // while a window opens.
            taskbar: crate::taskbar::Taskbar::load(),
            window_state: personal.then(crate::window_state::WindowState::load),
            worktree_landing: read(personal, crate::worktree_landing::Remembered::load),
            worktree_notes: read(personal, crate::worktree_notes::Notes::load),
        }
    }

    /// How many main windows the last session had open.
    pub(crate) fn window_count(&self) -> usize {
        self.window_state.as_ref().map_or(1, |state| state.count())
    }

    pub(crate) fn install(self, cx: &mut App) {
        if let Some(state) = self.window_state {
            state.install(cx);
        }
        self.browser_tabs.install(cx);
        self.group_layouts.install(cx);
        self.worktree_notes.install(cx);
        self.review_viewed.install(cx);
        self.overview_marks.install(cx);
        self.notification_mutes.install(cx);
        self.palette_frecency.install(cx);
        self.dispatch_history.install(cx);
        self.worktree_landing.install(cx);
        self.fan_out_journal.install(cx);
        self.agent_skill.install_global(cx);
        self.keychain_grants.install(cx);
        self.taskbar.install(cx);
        #[cfg(windows)]
        if let Some(jump_list) = self.jump_list {
            jump_list.install(cx);
        }
    }
}
