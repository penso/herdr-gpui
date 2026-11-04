//! A released selection offers annotation without taking the keyboard from
//! the terminal. The offer also survives copying that clears the highlight.
use super::*;

pub(super) struct Offer {
    pub(super) selection: Selection,
    origin: Origin,
    epoch: u64,
    checked: (u64, u64),
}

impl Offer {
    pub(super) fn current(&self, view: &SheprdWindow) -> bool {
        self.epoch == view.selection_epoch && self.origin.current(view) && view.live.surface_ready()
    }

    fn text_current(&self, view: &SheprdWindow) -> bool {
        let Some(surface) = &view.live.surface else {
            return false;
        };
        let height = view.config.terminal.line_height();
        surface.popup.is_none()
            && self
                .selection
                .rows(surface, view.cell_width, height)
                .next()
                .is_some()
            && self
                .selection
                .offscreen_range(surface, view.cell_width, height)
                .is_none()
            && self
                .selection
                .text_unchanged(surface, view.cell_width, height)
    }
}

impl SheprdWindow {
    pub(crate) fn offer_terminal_annotation(&mut self, cx: &mut Context<Self>) {
        // Copying another passage must not replace a message already being written.
        if self.terminal_notes.composer.is_some() || !self.live.surface_ready() {
            return;
        }
        let (Some(selection), Some(surface), Some(origin)) =
            (&self.selection, &self.live.surface, Origin::of(self))
        else {
            return;
        };
        if selection.dragging() || selection.pane_id().is_none() {
            return;
        }
        // A passive affordance stays silent for blanks, offscreen text, or a
        // selection too large to quote. Explicit annotation reports the reason.
        if !selection
            .text(surface, self.cell_width, self.config.terminal.line_height())
            .is_ok_and(|text| !text.trim().is_empty())
        {
            return;
        }
        self.terminal_notes.offer = Some(Offer {
            selection: selection.clone(),
            origin,
            epoch: self.selection_epoch,
            checked: (surface.projection_revision, surface.surface_revision),
        });
        cx.notify();
    }

    pub(crate) fn clear_terminal_annotation_offer(&mut self, cx: &mut Context<Self>) {
        if self.terminal_notes.offer.take().is_some() {
            cx.notify();
        }
    }

    pub(super) fn follow_terminal_annotation_offer(&mut self, cx: &mut Context<Self>) {
        let Some(offer) = &self.terminal_notes.offer else {
            return;
        };
        if !offer.current(self) {
            self.clear_terminal_annotation_offer(cx);
            return;
        }
        let Some(surface) = &self.live.surface else {
            return;
        };
        let revision = (surface.projection_revision, surface.surface_revision);
        if offer.checked == revision {
            return;
        }
        if !offer.text_current(self) {
            self.clear_terminal_annotation_offer(cx);
        } else if let Some(offer) = &mut self.terminal_notes.offer {
            offer.checked = revision;
        }
    }

    pub(super) fn open_terminal_annotation_offer(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(offer) = self.terminal_notes.offer.take() else {
            return;
        };
        // A mouse event can arrive before the next tick. Revalidate here too,
        // so a delayed click can never quote replacement cells or another host.
        if offer.current(self) && offer.text_current(self) {
            self.compose_terminal_selection(offer.selection, window, cx);
        }
        cx.notify();
    }
}
