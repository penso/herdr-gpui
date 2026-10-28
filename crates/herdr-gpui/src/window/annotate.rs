//! Annotating terminal text: a note field over the pane the user selected
//! text in, and the queue of notes waiting to go to that pane's agent. What a
//! note holds and the prompt it becomes live in [`crate::terminal_notes`].
//!
//! As with the find field, typing in the note field never reaches the
//! terminal: its key handler steps aside while the field has focus.

use super::{Flash, HerdrWindow};
use crate::{
    Error,
    agent_notes::Origin,
    search_input::SearchInput,
    terminal::Selection,
    terminal_notes::{self, MAX_NOTES, Note},
};
use gpui::{prelude::*, *};
use herdr_client::protocol::ClientShellSnapshot;

mod offer;
mod render;

/// A queued note and the daemon whose agent it goes to.
struct Queued {
    origin: Origin,
    note: Note,
}

/// The note being written, about text selected in `pane_id`.
struct Composer {
    input: Entity<SearchInput>,
    origin: Origin,
    pane_id: String,
    target: Option<String>,
    place: String,
    quote: String,
    selection: Selection,
}

/// The window's terminal notes: the one being written and those queued.
#[derive(Default)]
pub(crate) struct TerminalNotes {
    composer: Option<Composer>,
    offer: Option<offer::Offer>,
    queued: Vec<Queued>,
}

impl TerminalNotes {
    #[cfg(test)]
    pub(crate) fn queued(&self) -> usize {
        self.queued.len()
    }

    #[cfg(test)]
    pub(crate) fn composing(&self) -> bool {
        self.composer.is_some()
    }
}

/// The agent a note on `pane_id` goes to: the pane's own, or else the first in
/// its workspace. Also where the pane is, as `workspace / tab`.
fn destination(snapshot: &ClientShellSnapshot, pane_id: &str) -> (Option<String>, String) {
    let Some(pane) = snapshot.panes.iter().find(|pane| pane.pane_id == pane_id) else {
        return (None, String::new());
    };
    let target = crate::agent_notes::agent(snapshot, pane_id)
        .or_else(|| {
            snapshot
                .agents
                .iter()
                .find(|agent| agent.workspace_id == pane.workspace_id)
        })
        .map(|agent| agent.pane_id.clone());
    let workspace = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == pane.workspace_id)
        .map(|workspace| workspace.label.trim())
        .filter(|label| !label.is_empty());
    let tab = snapshot
        .tabs
        .iter()
        .find(|tab| tab.tab_id == pane.tab_id)
        .and_then(|tab| herdr_client::text::caption(&tab.label));
    let place = [workspace, tab].into_iter().flatten().collect::<Vec<_>>();
    (target, place.join(" / "))
}

impl HerdrWindow {
    /// Opens the note field over the pane holding the selection.
    pub(crate) fn annotate_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(selection) = self.selection.clone() else {
            if self.terminal_notes.offer.is_some() {
                self.open_terminal_annotation_offer(window, cx);
            } else {
                self.show_flash(Flash::warning("Select terminal text to annotate it"), cx);
            }
            return;
        };
        self.compose_terminal_selection(selection, window, cx);
    }

    fn compose_terminal_selection(
        &mut self,
        selection: Selection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(snapshot) = self.live.snapshot.clone() else {
            return;
        };
        let Some(surface) = &self.live.surface else {
            self.show_flash(Flash::warning("Select terminal text to annotate it"), cx);
            return;
        };
        let Some(pane_id) = selection.pane_id().map(str::to_owned) else {
            self.show_flash(Flash::warning("Only text in a pane can be annotated"), cx);
            return;
        };
        let quote =
            match selection.text(surface, self.cell_width, self.config.terminal.line_height()) {
                Ok(quote) if !quote.trim().is_empty() => quote,
                Ok(_) => {
                    self.show_flash(Flash::warning("Select terminal text to annotate it"), cx);
                    return;
                }
                Err(Error::SelectionOffscreen) => {
                    self.show_flash(
                        Flash::warning("Scroll so the whole selection shows, then annotate it"),
                        cx,
                    );
                    return;
                }
                Err(error) => {
                    self.show_flash(Flash::warning(format!("Cannot annotate: {error}")), cx);
                    return;
                }
            };
        let (target, place) = destination(&snapshot, &pane_id);
        let input = cx.new(SearchInput::new);
        input.update(cx, |input, cx| {
            input.set_placeholder("Message to agent\u{2026}", cx);
            input.set_frameless(cx);
            let mut font = self.config.ui.clone();
            font.size = font.size.max(14.);
            input.set_appearance(font, self.theme.clone(), cx);
        });
        let focus = input.read(cx).focus.clone();
        let Some(origin) = Origin::of(self) else {
            return;
        };
        self.terminal_notes.offer = None;
        self.terminal_notes.composer = Some(Composer {
            input,
            origin,
            pane_id,
            target,
            place,
            quote,
            selection,
        });
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Drops the note being written and hands the keyboard back.
    pub(crate) fn close_terminal_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(composer) = self.terminal_notes.composer.take() else {
            return;
        };
        if composer.input.read(cx).focus.is_focused(window) {
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }

    /// Whether the note field holds the keyboard, so the terminal must not
    /// act on a keystroke bubbling out of it.
    pub(crate) fn terminal_note_focused(&self, window: &Window, cx: &App) -> bool {
        self.terminal_notes
            .composer
            .as_ref()
            .is_some_and(|composer| composer.input.read(cx).focus.is_focused(window))
    }

    /// Runs every tick: retires the field when its pane or connection is gone
    /// or its pane left the screen.
    pub(crate) fn follow_terminal_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.follow_terminal_annotation_offer(cx);
        let Some(composer) = &self.terminal_notes.composer else {
            return;
        };
        let current = composer.origin.current(self)
            && self.live.snapshot.as_ref().is_some_and(|snapshot| {
                snapshot
                    .panes
                    .iter()
                    .any(|pane| pane.pane_id == composer.pane_id)
            })
            && (!self.live.surface_ready()
                || self.live.surface.as_deref().is_some_and(|surface| {
                    surface.popup.is_none()
                        && surface
                            .panes
                            .iter()
                            .any(|pane| pane.pane_id == composer.pane_id)
                }));
        if !current {
            self.close_terminal_note(window, cx);
        }
    }

    /// Queues the note being written, then sends every queued note when
    /// `send` says so. An empty field still sends what is queued.
    pub(crate) fn add_terminal_note(
        &mut self,
        send: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(composer) = &self.terminal_notes.composer else {
            return;
        };
        let comment = composer.input.read(cx).text().to_owned();
        let note = Note::new(
            composer.target.clone(),
            &composer.place,
            &composer.quote,
            &comment,
        );
        match note {
            Some(note) => {
                if self.terminal_notes.queued.len() >= MAX_NOTES {
                    self.show_flash(
                        Flash::warning(format!(
                            "{MAX_NOTES} notes are queued; clear the field and press Enter to send them"
                        )),
                        cx,
                    );
                    return;
                }
                let origin = composer.origin.clone();
                self.terminal_notes.queued.push(Queued { origin, note });
            }
            None if send && !self.terminal_notes.queued.is_empty() => {}
            None => {
                self.show_flash(Flash::warning("Write a note before adding it"), cx);
                return;
            }
        }
        self.close_terminal_note(window, cx);
        self.clear_retained_selection(cx);
        if send {
            self.send_terminal_notes(cx);
        } else {
            let count = self.terminal_notes.queued.len();
            self.show_flash(
                Flash::success(format!(
                    "{count} note{} queued; Enter on the next one sends them",
                    if count == 1 { "" } else { "s" }
                )),
                cx,
            );
        }
    }

    /// Sends the queued notes, one batch per agent, in the order they were
    /// written.
    pub(crate) fn send_terminal_notes(&mut self, cx: &mut Context<Self>) {
        let clipboard = crate::agent_notes::Copies::default();
        let (mut queued, copied): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.terminal_notes.queued)
                .into_iter()
                .partition(|queued| queued.origin.current(self) && queued.note.target.is_some());
        while let Some(first) = queued.first() {
            let (origin, target) = (first.origin.clone(), first.note.target.clone());
            let (batch, rest): (Vec<_>, Vec<_>) = queued
                .into_iter()
                .partition(|queued| queued.origin == origin && queued.note.target == target);
            queued = rest;
            let text = terminal_notes::prompt(batch.iter().map(|queued| &queued.note));
            let target = target.map(|pane_id| crate::browser::FeedbackKey {
                scope: crate::browser::scope(&self.endpoints[self.selected_endpoint]),
                pane_id,
            });
            clipboard.extend(
                self.deliver_notes(target, text, None, Some(clipboard.clone()), cx)
                    .copy(),
            );
        }
        if !copied.is_empty() {
            // Feedback is keyed by daemon, not session: once the session a
            // note was written in is gone, its pane ID may name another
            // pane, so even the fallback could reach the wrong agent.
            let stale = copied.iter().any(|queued| !queued.origin.current(self));
            let text = terminal_notes::prompt(copied.iter().map(|queued| &queued.note));
            clipboard.extend(
                self.deliver_notes(None, text, None, Some(clipboard.clone()), cx)
                    .copy(),
            );
            if stale {
                self.show_flash(
                    Flash::warning("Notes whose original daemon is not selected were copied"),
                    cx,
                );
            }
        }
        if let Some(text) = clipboard.text() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    /// Keys the field leaves alone: Escape drops the note, Enter adds it and
    /// sends the queue, Shift-Enter only queues it.
    fn terminal_note_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(composer) = &self.terminal_notes.composer else {
            return;
        };
        if composer.input.read(cx).is_composing() {
            return;
        }
        let modifiers = event.keystroke.modifiers;
        match event.keystroke.key.as_str() {
            "escape" if !modifiers.modified() => self.close_terminal_note(window, cx),
            "enter" if modifiers.shift => self.add_terminal_note(false, window, cx),
            "enter" if !modifiers.modified() => self.add_terminal_note(true, window, cx),
            _ => return,
        }
        cx.stop_propagation();
        window.prevent_default();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests;
