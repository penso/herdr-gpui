//! Visible chat panes share a window's bounded visual workers.

use crate::{
    SheprdWindow,
    browser::Shown,
    chat::{
        Target,
        visual::{self, DiagramStyle, Job, Output, VisualId},
    },
};
use gpui::{App, AppContext, Context, Window};
use std::rc::Rc;

impl SheprdWindow {
    /// Starts visible chats' queued visuals and releases unused textures.
    pub(in crate::chat) fn prepare_chat_visuals(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for texture in self.chats.take_released() {
            cx.drop_image(texture, Some(window));
        }
        #[cfg(any(target_os = "macos", windows))]
        for page in self.chats.take_closed_pages() {
            self.close_chat_page(page);
        }
        for pane in self.chats.panes_mut() {
            for texture in pane.prepared.visuals.take_released() {
                cx.drop_image(texture, Some(window));
            }
        }
        for (target, generation, job) in self.take_chat_visual_jobs(cx) {
            let task = cx.background_spawn(async move { job.run() });
            cx.spawn(async move |this, cx| {
                let (id, ticket, drawn) = task.await;
                let _ = this.update(cx, |this, cx| {
                    this.finish_chat_visual((&target, &generation), id, ticket, drawn, cx);
                });
            })
            .detach();
        }
    }

    /// Claim jobs before spawning, so split panes and late detached results
    /// cannot exceed the same window-wide budget.
    pub(in crate::chat) fn take_chat_visual_jobs(
        &mut self,
        cx: &App,
    ) -> Vec<(Target, Rc<()>, Job)> {
        if !self.config.features.acp_chats {
            return Vec::new();
        }
        let primary = self.primary_group();
        let groups = self.drawn_group_slots();
        let herdr_visible = self.live.surface_ready()
            && groups.iter().any(|slot| {
                primary.is_none_or(|group| group == slot.id)
                    && self.group_shown(slot.id, cx) == Shown::Terminal
            });
        let mut visible: Vec<_> = self
            .live
            .surface
            .as_ref()
            .filter(|surface| herdr_visible && surface.popup.is_none())
            .into_iter()
            .flat_map(|surface| surface.panes.iter().map(move |pane| (pane, surface)))
            .filter(|(pane, surface)| {
                let rect = self
                    .chat_terminal_layout(pane, surface)
                    .map_or(pane.inner_rect, |layout| layout.conversation);
                rect.width > 0 && rect.height > 0
            })
            .map(|(pane, _)| pane.pane_id.clone())
            .collect();
        // Core chats are pages with their own split panes. They need no
        // Herdr 1 surface, and a hidden or mirrored tab must not admit work.
        for slot in groups {
            let Shown::Page(tab) = self.group_shown(slot.id, cx) else {
                continue;
            };
            for terminal in self.core_terminals.visible_chat_panes(tab) {
                let key = crate::chat::core_pane::key(terminal);
                if !visible.contains(&key) {
                    visible.push(key);
                }
            }
        }
        visible.retain(|pane| self.chats.shown(pane));
        let mut font = self.config.ui.clone();
        font.size = self.chat_text_size();
        let style = DiagramStyle::of(&self.theme, &font);
        let renderer = cx.svg_renderer();
        let mut pending = Vec::new();
        if visible.is_empty() {
            return pending;
        }
        // Advance only after admission, including across completion callbacks:
        // a constantly busy first split cannot starve a later visible pane.
        let start = self.chats.visual_cursor % visible.len();
        for offset in 0..visible.len() * visual::MAX_RUNNING {
            if self.chats.active_visual_jobs >= visual::MAX_RUNNING {
                break;
            }
            let index = (start + offset) % visible.len();
            let Some(pane) = self.chats.pane_mut(&visible[index]) else {
                continue;
            };
            let jobs = pane.prepared.visuals.jobs_with_limit(&style, &renderer, 1);
            let started = jobs.len();
            for job in jobs {
                pending.push((
                    pane.target.clone(),
                    pane.prepared.visuals.generation.clone(),
                    job,
                ));
            }
            self.chats.active_visual_jobs += started;
            if started > 0 {
                self.chats.visual_cursor = (index + 1) % visible.len();
            }
        }
        pending
    }

    pub(in crate::chat) fn finish_chat_visual(
        &mut self,
        (target, generation): (&Target, &Rc<()>),
        id: VisualId,
        ticket: u64,
        drawn: visual::Result<Output>,
        cx: &mut Context<Self>,
    ) {
        self.chats.active_visual_jobs -= 1;
        // Even a removed pane's result frees a slot for current visible work.
        self.redraw_terminal(cx);
        let Some(pane) = self.chats.pane_mut(&target.pane_id) else {
            return;
        };
        if pane.target != *target || !Rc::ptr_eq(&pane.prepared.visuals.generation, generation) {
            return;
        }
        if !pane.prepared.visuals.finish(id, ticket, drawn) {
            return;
        }
        let entries: Vec<_> = pane.prepared.drawing(id).collect();
        for entry in entries {
            pane.remeasure(entry);
        }
    }
}
