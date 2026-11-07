//! The chats a window shows share its visual workers: claimed here on the
//! UI thread, run on the background executor, and handed back to their pane.

use crate::{
    SheprdWindow,
    browser::Shown,
    chat::{
        core_pane,
        visual::{self, DiagramStyle, Job, Output, VisualId},
    },
};
use gpui::{App, AppContext as _, Context, Window};
use std::rc::Rc;

/// A job that holds a worker slot, and the pane it was started for.
pub(in crate::chat) struct Claimed {
    pub(in crate::chat) generation: Rc<()>,
    pub(in crate::chat) job: Job,
    pub(in crate::chat) pane_id: String,
}

impl SheprdWindow {
    /// Starts the visuals the shown chats wait for on the background
    /// executor, and drops the textures no entry draws any more.
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
        for claimed in self.claim_chat_visuals(cx) {
            let Claimed {
                generation,
                job,
                pane_id,
            } = claimed;
            let task = cx.background_spawn(async move { job.run() });
            cx.spawn(async move |this, cx| {
                let (id, ticket, drawn) = task.await;
                let _ = this.update(cx, |this, cx| {
                    this.finish_chat_visual((&pane_id, &generation), id, ticket, drawn, cx);
                });
            })
            .detach();
        }
    }

    /// The chat panes on screen: the window's terminal group's, then those
    /// of the herdr-core tabs shown. A popup covers the terminal's.
    fn visible_chat_panes(&self, cx: &App) -> Vec<String> {
        let primary = self.primary_group();
        let groups = self.drawn_group_slots();
        let terminal_shown = self.live.surface_ready()
            && groups.iter().any(|slot| {
                primary.is_none_or(|group| group == slot.id)
                    && self.group_shown(slot.id, cx) == Shown::Terminal
            });
        let mut visible: Vec<String> = self
            .live
            .surface
            .as_ref()
            .filter(|surface| terminal_shown && surface.popup.is_none())
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
        for slot in groups {
            let Shown::Page(tab) = self.group_shown(slot.id, cx) else {
                continue;
            };
            for terminal in self.core_terminals.visible_chat_panes(tab) {
                let key = core_pane::key(terminal);
                if !visible.contains(&key) {
                    visible.push(key);
                }
            }
        }
        visible.retain(|pane| self.chats.shown(pane));
        visible
    }

    /// Takes the window's free worker slots for the panes on screen, one job
    /// each in turn, so a busy split cannot starve the next. Slots are held
    /// from here until [`Self::finish_chat_visual`], whatever became of the
    /// pane, so hidden or removed work still counts.
    pub(in crate::chat) fn claim_chat_visuals(&mut self, cx: &App) -> Vec<Claimed> {
        let free = visual::MAX_RUNNING.saturating_sub(self.chats.active_visual_jobs);
        if free == 0 || !self.config.features.acp_chats {
            return Vec::new();
        }
        let visible = self.visible_chat_panes(cx);
        if visible.is_empty() {
            return Vec::new();
        }
        let font = self.chat_font();
        let style = DiagramStyle::of(&self.theme, &font);
        let renderer = cx.svg_renderer();
        let start = self.chats.visual_cursor % visible.len();
        let mut claimed = Vec::new();
        // Each pane holds at most `MAX_RUNNING`, so that many rounds offer
        // every pane all it could take.
        for offset in 0..visible.len() * visual::MAX_RUNNING {
            if claimed.len() >= free {
                break;
            }
            let index = (start + offset) % visible.len();
            let Some(pane) = self.chats.pane_mut(&visible[index]) else {
                continue;
            };
            let before = claimed.len();
            claimed.extend(
                pane.prepared
                    .visuals
                    .jobs_within(1, &style, &renderer)
                    .into_iter()
                    .map(|job| Claimed {
                        generation: pane.prepared.visuals.generation.clone(),
                        job,
                        pane_id: pane.target.pane_id.clone(),
                    }),
            );
            if claimed.len() > before {
                self.chats.visual_cursor = (index + 1) % visible.len();
            }
        }
        self.chats.active_visual_jobs += claimed.len();
        claimed
    }

    /// Frees a job's worker slot and gives its pane the result, unless that
    /// pane has gone, even for one recreated under the same name.
    pub(in crate::chat) fn finish_chat_visual(
        &mut self,
        (pane_id, generation): (&str, &Rc<()>),
        id: VisualId,
        ticket: u64,
        drawn: visual::Result<Output>,
        cx: &mut Context<Self>,
    ) {
        self.chats.active_visual_jobs = self.chats.active_visual_jobs.saturating_sub(1);
        let Some(pane) = self
            .chats
            .pane_mut(pane_id)
            .filter(|pane| Rc::ptr_eq(&pane.prepared.visuals.generation, generation))
        else {
            return;
        };
        if !pane.prepared.visuals.finish(id, ticket, drawn) {
            return;
        }
        // The row's height changed with what it now draws.
        let entries: Vec<_> = pane.prepared.drawing(id).collect();
        for entry in entries {
            pane.remeasure(entry);
        }
        self.redraw_terminal(cx);
    }
}
