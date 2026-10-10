//! Sending a tab's notes to the agent that opened its page, through the
//! shared delivery in `agent_notes`, or copying them.

use super::{
    super::{Tab, annotate},
    screenshots::save_screenshots,
};
use crate::{HerdrWindow, window::Flash};
use gpui::{prelude::*, *};
use std::{path::PathBuf, sync::Arc};

impl HerdrWindow {
    /// The notes Send delivers, with their places in the list: those not
    /// sent yet, or all of them again.
    fn notes_round(&mut self, tab: &Tab) -> (Vec<usize>, Vec<annotate::Note>) {
        let notes = &self.tab_notes(tab.id).notes;
        let indexes = crate::agent_notes::round(notes.iter().map(|note| note.sent));
        let round = indexes.iter().map(|&index| notes[index].clone()).collect();
        (indexes, round)
    }

    /// `notes` as a prompt, after their screenshots are saved to files the
    /// agent can read. Saving runs off the UI thread; `then` gets the prompt
    /// back on it.
    fn with_notes_prompt(
        &mut self,
        tab: &Tab,
        notes: Vec<annotate::Note>,
        save: impl FnOnce(&[Option<Arc<Image>>]) -> crate::Result<Vec<Option<PathBuf>>> + Send + 'static,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, String, &mut Context<Self>) + 'static,
    ) {
        if notes.is_empty() {
            return;
        }
        let reload = crate::control::reload_command();
        if notes.iter().all(|note| note.image.is_none()) {
            let text = annotate::prompt(tab, &notes, &[], &reload);
            then(self, text, cx);
            return;
        }
        let images: Vec<Option<Arc<Image>>> = notes.iter().map(|note| note.image.clone()).collect();
        let saving = cx.background_executor().spawn(async move { save(&images) });
        let tab = tab.clone();
        cx.spawn(async move |this, cx| {
            let paths = saving.await;
            this.update(cx, |this, cx| {
                let paths = paths.unwrap_or_else(|error| {
                    tracing::warn!(%error, "Could not save note screenshots");
                    this.show_flash(Flash::warning("Screenshots could not be saved"), cx);
                    Vec::new()
                });
                let text = annotate::prompt(&tab, &notes, &paths, &reload);
                then(this, text, cx);
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn copy_notes(&mut self, tab: &Tab, cx: &mut Context<Self>) {
        let (_, notes) = self.notes_round(tab);
        self.with_notes_prompt(tab, notes, save_screenshots, cx, |this, text, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            this.show_flash(Flash::success("Notes copied"), cx);
        });
    }

    /// Sends the notes not sent yet, or all again, to the agent that opened
    /// the page: to it directly when it waits in `browser feedback`,
    /// otherwise into its pane once it is idle, and kept for `browser
    /// feedback` when its pane is gone.
    pub(in crate::browser) fn send_notes(&mut self, tab: &Tab, cx: &mut Context<Self>) {
        self.send_notes_with(tab, save_screenshots, cx);
    }

    fn send_notes_with(
        &mut self,
        tab: &Tab,
        save: impl FnOnce(&[Option<Arc<Image>>]) -> crate::Result<Vec<Option<PathBuf>>> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(pending) = self.tab_notes(tab.id).sending.start() else {
            self.show_flash(
                Flash::warning("The previous notes are still pending delivery"),
                cx,
            );
            return;
        };
        let (indexes, notes) = self.notes_round(tab);
        let pane_id = tab.origin.clone();
        // The page's daemon must still be the one shown, in the session
        // shown now, once the screenshots are saved.
        let origin = crate::agent_notes::Origin::of(self).filter(|_| {
            crate::browser::scope(&self.endpoints[self.selected_endpoint]) == tab.scope
        });
        self.with_notes_prompt(tab, notes, save, cx, move |this, text, cx| {
            if let Some(text) = this
                .deliver_from(origin.as_ref(), pane_id, text, Some(pending), cx)
                .copy()
            {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
            }
        });
        self.mark_notes_sent(tab.id, &indexes, cx);
    }
}

#[cfg(test)]
mod tests;
