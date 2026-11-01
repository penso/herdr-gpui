//! Selecting terminal cells with the pointer and copying them. The gesture
//! never sends input to the daemon and writes the clipboard only when the user
//! releases a selection they made or asks to copy one still highlighted. A drag held past a pane's top or bottom
//! scrolls the pane, and a selection reaching rows the pane no longer shows is
//! read back with `pane.selection.read`; everything else is copied from the
//! painted cells.
// i18n: migrated

use super::HerdrWindow;
use crate::i18n::{Localize, fl};
use crate::{scrollback::Inbox, terminal::Selection};
use gpui::{ClipboardItem, Context, Pixels, Point};
use herdr_client::{
    Method,
    scrollback::{ScrollbackResponse, SelectionReadParams},
};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

/// How often a drag held past a pane's edge scrolls it by another step.
const AUTOSCROLL_INTERVAL: Duration = Duration::from_millis(50);

/// How many of its newest offsets a drag remembers while frames fall behind
/// its steps. A frame that lags further than this starts the next step from
/// what it shows.
const AUTOSCROLL_LAG: usize = 32;

/// What a drag needs beyond the selection itself: where the pointer last was,
/// so the selection can follow the pane as it scrolls under a still pointer,
/// the last scroll it asked for, the last frame a kept highlight was checked
/// against, and the copy the daemon is reading for a released selection.
#[derive(Default)]
pub(crate) struct Follow {
    /// Shift at the start offers annotation, even when released before the mouse.
    offer_annotation: bool,
    pointer: Option<Point<Pixels>>,
    scrolled: Option<Instant>,
    /// The drag's last `pane.scroll`. The daemon's answer can arrive before
    /// the frame that shows it, so the next step continues from here instead
    /// of repeating this one or turning back from an older offset.
    sent: Option<Sent>,
    /// The projection and surface revisions a kept highlight was last checked
    /// against, so an unchanged frame is not hashed again every tick.
    checked: Option<(u64, u64)>,
    read: Option<(Arc<Mutex<Inbox>>, String)>,
}

/// The `pane.scroll` steps a drag sent since a frame last showed the latest one.
struct Sent {
    pane_id: String,
    /// The offset on screen before the first of these steps, then each step
    /// in order: a frame showing one of them has not caught up yet.
    path: Vec<u64>,
}

impl Sent {
    /// The last step asked for, if a frame showing `shown` in `pane_id` is
    /// still behind it.
    fn ahead_of(&self, pane_id: &str, shown: u64) -> Option<u64> {
        let last = *self.path.last()?;
        (self.pane_id == pane_id && self.path.contains(&shown)).then_some(last)
    }

    /// Records a step to `offset`, sent while the frame showed `shown`.
    fn record(sent: &mut Option<Self>, pane_id: String, shown: u64, offset: u64) {
        match sent {
            Some(sent)
                if sent
                    .ahead_of(&pane_id, shown)
                    .is_some_and(|last| last != shown) =>
            {
                if sent.path.len() == AUTOSCROLL_LAG {
                    sent.path.remove(0);
                }
                sent.path.push(offset);
            }
            _ => {
                *sent = Some(Self {
                    pane_id,
                    path: vec![shown, offset],
                });
            }
        }
    }
}

impl HerdrWindow {
    /// Starts a selection under the pointer, discarding the previous one. A
    /// double click starts on the link or word under it and a triple click on
    /// its row. A press that lands outside the painted cells only clears.
    pub(crate) fn begin_selection(
        &mut self,
        position: Point<Pixels>,
        clicks: usize,
        offer_annotation: bool,
        cx: &mut Context<Self>,
    ) {
        // A click hands the terminal back from copy mode.
        self.leave_copy_mode(cx);
        let cleared = self.selection.take().is_some();
        self.clear_terminal_annotation_offer(cx);
        self.selection_follow.offer_annotation = offer_annotation;
        self.selection_follow.sent = None;
        if let Some(surface) = self.selectable_surface(position) {
            let (x, y) = self.grid_position(position);
            self.selection = Selection::begin(
                surface,
                x,
                y,
                self.cell_width,
                self.config.terminal.line_height(),
                clicks,
            );
        }
        if cleared || self.selection.is_some() {
            cx.notify();
        }
    }

    /// Follows the pointer while the button that started the drag is down.
    /// Returns whether a drag is in progress, so the caller can keep the
    /// gesture to itself.
    pub(crate) fn extend_selection(
        &mut self,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> bool {
        let (x, y) = self.grid_position(position);
        let cell_width = self.cell_width;
        let cell_height = self.config.terminal.line_height();
        let (Some(selection), Some(surface)) = (&mut self.selection, &self.live.surface) else {
            return false;
        };
        if !selection.dragging() {
            return false;
        }
        self.selection_follow.pointer = Some(position);
        if selection.extend(surface, x, y, cell_width, cell_height) {
            cx.notify();
        }
        true
    }

    /// Herdr's shared `ui.copy_on_select`, on until the shared config loads.
    pub(super) fn copy_on_select(&self) -> bool {
        self.settings
            .shared
            .as_ref()
            .is_none_or(|shared| shared.copy_on_select)
    }

    /// Ends a drag: a Shift-selection offers annotation, and what it chose
    /// goes to the clipboard with a flash when copying on selection is on.
    /// The highlight stays while `keep_selection_after_copy` is on, so tools
    /// that read the selection find it and Cmd-C copies it again. With Herdr's
    /// `copy_on_select` off, nothing is copied yet and the highlight stays for
    /// [`Self::copy_retained_selection`]. Returns
    /// whether the release belonged to the selection, since a press that
    /// chose no cells is still the click that opens a link under the pointer.
    pub(crate) fn release_selection(&mut self, cx: &mut Context<Self>) -> bool {
        if !self
            .selection
            .as_mut()
            .is_some_and(|selection| selection.release())
        {
            return false;
        }
        self.selection_follow.pointer = None;
        let cell_height = self.config.terminal.line_height();
        if let (Some(selection), Some(surface)) =
            (&mut self.selection, self.live.surface.as_deref())
        {
            selection.remember_text(surface, self.cell_width, cell_height);
        }
        let selected = !self.selection_is_empty();
        if selected && self.selection_follow.offer_annotation {
            // Keep the selection readable by the pill even if copying clears
            // its highlight. Offering annotation never takes keyboard focus.
            self.offer_terminal_annotation(cx);
        }
        if selected && !self.copy_on_select() {
            cx.notify();
            return true;
        }
        self.finish_copy(selected, cx);
        selected
    }

    /// Whether a released selection is still highlighted, waiting for an
    /// explicit copy or kept after one.
    pub(crate) fn selection_retained(&self) -> bool {
        self.selection
            .as_ref()
            .is_some_and(|selection| !selection.dragging())
            && !self.selection_is_empty()
    }

    /// Drops a kept highlight once the terminal writes other text into the
    /// rows it marks, as terminal emulators do, rather than leaving it over
    /// cells the user never chose, where a later copy would take them.
    fn retire_overwritten_selection(&mut self, cx: &mut Context<Self>) {
        let cell_height = self.config.terminal.line_height();
        let (Some(selection), Some(surface)) = (&self.selection, self.live.surface.as_deref())
        else {
            return;
        };
        let revision = (surface.projection_revision, surface.surface_revision);
        if selection.dragging() || self.selection_follow.checked.replace(revision) == Some(revision)
        {
            return;
        }
        if !selection.text_unchanged(surface, self.cell_width, cell_height) {
            self.selection = None;
            cx.notify();
        }
    }

    /// Copies a selection kept on release, as Herdr's Ctrl-C or Cmd-C does,
    /// and clears it unless `keep_selection_after_copy` is on. `false` when no
    /// released selection is waiting.
    pub(crate) fn copy_retained_selection(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.selection_retained() {
            return false;
        }
        self.finish_copy(true, cx);
        true
    }

    /// Cmd-A and Edit > Select All: selects the focused pane's history and
    /// screen, or the open popup, as a released selection that Cmd-C copies.
    /// Nothing reaches the clipboard yet, and nothing reaches the daemon until
    /// that copy reads the rows the pane does not show. `false` when the
    /// terminal shows nothing to select.
    pub(crate) fn select_all(&mut self, cx: &mut Context<Self>) -> bool {
        if self.copy_mode_active() || self.menu.page.is_some() || !self.live.surface_ready() {
            return false;
        }
        let Some((target, surface)) = self
            .focused_input_target()
            .zip(self.live.surface.as_deref())
        else {
            return false;
        };
        let Some(selection) = Selection::all(
            surface,
            target,
            self.cell_width,
            self.config.terminal.line_height(),
        ) else {
            return false;
        };
        self.clear_terminal_annotation_offer(cx);
        self.selection_follow.offer_annotation = false;
        self.selection_follow.pointer = None;
        self.selection_follow.sent = None;
        self.selection = Some(selection);
        cx.notify();
        true
    }

    /// Drops a selection kept on release, as any other key does in Herdr.
    pub(crate) fn clear_retained_selection(&mut self, cx: &mut Context<Self>) {
        self.clear_terminal_annotation_offer(cx);
        if self
            .selection
            .as_ref()
            .is_some_and(|selection| !selection.dragging())
        {
            self.selection = None;
            cx.notify();
        }
    }

    fn finish_copy(&mut self, selected: bool, cx: &mut Context<Self>) {
        // A kept selection stays readable by selection tools and screen
        // readers until the next click or keystroke. Otherwise the gesture is
        // over either way: nothing stays highlighted behind it.
        let keep = selected && self.config.keep_selection_after_copy;
        if selected && self.read_offscreen_selection() {
            if !keep {
                self.selection = None;
            }
            cx.notify();
            return;
        }
        let copied = selected && self.copy_selection(cx);
        if !keep {
            self.selection = None;
        }
        if copied && self.config.clipboard_toast.enabled {
            self.show_flash(super::Flash::success(fl!("window-find-copied")), cx);
        }
        cx.notify();
    }

    /// Asks the daemon for a selection that reaches rows the pane does not
    /// show. `false` when the painted cells hold all of it.
    fn read_offscreen_selection(&mut self) -> bool {
        let cell_height = self.config.terminal.line_height();
        let (Some(selection), Some(surface)) = (&self.selection, self.live.surface.as_deref())
        else {
            return false;
        };
        let Some((pane_id, range)) =
            selection.offscreen_range(surface, self.cell_width, cell_height)
        else {
            return false;
        };
        let params = SelectionReadParams {
            pane_id: pane_id.to_owned(),
            anchor: range.start,
            cursor: range.end,
            // An explicit selection reads the live terminal: output since the
            // frame on screen must not refuse the copy.
            content_revision: None,
        };
        let connection = &self.endpoints[self.selected_endpoint].connection;
        let result = if !self.live.supports_selection_read {
            Err(crate::Error::SelectionOffscreen)
        } else if let (Some(handle), Some(snapshot)) = (&connection.handle, &self.live.snapshot) {
            let inbox = connection.scrollback.clone();
            let sent = inbox
                .try_lock()
                .map_err(|_| crate::Error::ConnectionBusy)
                .and_then(|mut mailbox| {
                    Ok(mailbox.send(|| handle.read_selection(&snapshot.boot_id, &params))?)
                });
            sent.map(|request| (inbox, request))
        } else {
            Err(crate::Error::NotConnected)
        };
        match result {
            Ok((inbox, request)) => self.await_selection_read(inbox, request),
            Err(error) => {
                self.local_error = Some(fl!(
                    "window-find-selection-not-copied",
                    error = error.localize()
                ))
            }
        }
        true
    }

    /// Copies what `request` reads once it comes back. A read still waiting
    /// is given up: the newer selection is the one the user wants.
    pub(super) fn await_selection_read(&mut self, inbox: Arc<Mutex<Inbox>>, request: String) {
        if let Some((inbox, request)) = self.selection_follow.read.replace((inbox, request))
            && let Ok(mut inbox) = inbox.try_lock()
        {
            inbox.discard(&request);
        }
    }

    /// Runs every tick while a drag or a daemon read is under way: scrolls a
    /// pane the pointer is held past, keeps the selection under the pointer
    /// as the pane moves, and copies a read that has come back.
    pub(crate) fn follow_selection(&mut self, cx: &mut Context<Self>) {
        self.finish_selection_read(cx);
        self.retire_overwritten_selection(cx);
        let Some(pointer) = self.selection_follow.pointer else {
            return;
        };
        if !self.selection.as_ref().is_some_and(Selection::dragging) {
            self.selection_follow.pointer = None;
            return;
        }
        // The pane may have scrolled under a pointer that has not moved.
        self.extend_selection(pointer, cx);
        let now = Instant::now();
        if self.live.drag_request.is_some()
            || self
                .selection_follow
                .scrolled
                .is_some_and(|last| now.saturating_duration_since(last) < AUTOSCROLL_INTERVAL)
        {
            return;
        }
        let Some((pane_id, offset, shown)) = self.selection_autoscroll(pointer) else {
            return;
        };
        let connection = &self.endpoints[self.selected_endpoint].connection;
        let (Some(handle), Some(snapshot)) = (&connection.handle, &self.live.snapshot) else {
            return;
        };
        // One scroll in flight at a time, registered like a scrollbar drag's.
        let Ok(mut state) = connection.inbox.try_lock() else {
            return;
        };
        match handle.request(
            &snapshot.boot_id,
            Method::PaneScroll,
            serde_json::json!({"pane_id": pane_id, "offset_from_bottom": offset}),
        ) {
            Ok(request) => {
                state.drag_request = Some(request.clone());
                self.live.drag_request = Some(request);
                self.selection_follow.scrolled = Some(now);
                Sent::record(&mut self.selection_follow.sent, pane_id, shown, offset);
            }
            Err(error) => {
                drop(state);
                self.local_error = Some(fl!(
                    "window-find-selection-scroll-failed",
                    error = error.localize()
                ));
                self.selection_follow.pointer = None;
                cx.notify();
            }
        }
    }

    /// The pane under a pane selection and the offset one step further in the
    /// direction the pointer is held past its edge: a row per row the pointer
    /// is out, up to a pane height, so a pointer held outside the window pages.
    /// Also returns the offset on screen, to tell later when a frame has moved.
    fn selection_autoscroll(&self, pointer: Point<Pixels>) -> Option<(String, u64, u64)> {
        let selection = self.selection.as_ref()?;
        let pane_id = selection.pane_id()?;
        let surface = self.live.surface.as_deref()?;
        let pane = surface.panes.iter().find(|pane| pane.pane_id == pane_id)?;
        let scroll = pane.scroll?;
        let cell_height = self.config.terminal.line_height();
        let (_, y) = self.grid_position(pointer);
        let top = f32::from(pane.inner_rect.y) * cell_height;
        let bottom = top + f32::from(pane.inner_rect.height) * cell_height;
        let page = u64::from(pane.inner_rect.height).max(1);
        let rows = |distance: f32| ((distance / cell_height).ceil() as u64).clamp(1, page);
        // The last request while the frame shows one it has not caught up
        // with: answers can arrive before their frames. A frame anywhere else
        // was moved by something else, such as a wheel.
        let shown = scroll.offset_from_bottom;
        let from = self
            .selection_follow
            .sent
            .as_ref()
            .and_then(|sent| sent.ahead_of(pane_id, shown))
            .unwrap_or(shown);
        let offset = if y < top {
            from.saturating_add(rows(top - y))
                .min(scroll.max_offset_from_bottom)
        } else if y >= bottom {
            from.saturating_sub(rows(y - bottom + 1.))
        } else {
            return None;
        };
        (offset != from).then(|| (pane_id.to_owned(), offset, shown))
    }

    /// Copies a selection the daemon has read back, once.
    fn finish_selection_read(&mut self, cx: &mut Context<Self>) {
        let Some((inbox, request)) = &self.selection_follow.read else {
            return;
        };
        let answer = match inbox.try_lock() {
            Ok(mut inbox) => inbox.take(request),
            Err(_) => return,
        };
        let Some(answer) = answer else {
            // A replaced connection never answers its old mailbox.
            if !Arc::ptr_eq(
                inbox,
                &self.endpoints[self.selected_endpoint].connection.scrollback,
            ) {
                self.selection_follow.read = None;
            }
            return;
        };
        self.selection_follow.read = None;
        match answer {
            Ok(ScrollbackResponse::PaneSelection(selection))
                if selection.text.len() <= crate::terminal::MAX_SELECTION_BYTES =>
            {
                cx.write_to_clipboard(ClipboardItem::new_string(selection.text));
                if self.config.clipboard_toast.enabled {
                    self.show_flash(super::Flash::success(fl!("window-find-copied")), cx);
                }
            }
            Ok(ScrollbackResponse::PaneSelection(_)) => {
                self.local_error = Some(fl!(
                    "window-find-selection-not-copied",
                    error = crate::Error::SelectionSize.localize()
                ));
            }
            Ok(_) => {
                self.local_error = Some(fl!(
                    "window-find-selection-not-copied",
                    error = herdr_client::Error::ResponseType.localize()
                ));
            }
            Err(error) => {
                self.local_error = Some(fl!(
                    "window-find-selection-not-copied",
                    error = error.localize()
                ))
            }
        }
        cx.notify();
    }

    /// Writes the current selection to the clipboard, reporting whether the
    /// text got there.
    fn copy_selection(&mut self, cx: &mut Context<Self>) -> bool {
        let text = {
            let (Some(selection), Some(surface)) = (&self.selection, &self.live.surface) else {
                return false;
            };
            selection.text(surface, self.cell_width, self.config.terminal.line_height())
        };
        match text {
            Ok(text) => {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                true
            }
            Err(error) => {
                self.local_error = Some(fl!(
                    "window-find-selection-not-copied",
                    error = error.localize()
                ));
                false
            }
        }
    }

    /// Whether the selection covers no cell the client can still show, either
    /// because the pointer never left the half-cell it pressed in or because
    /// the surface behind it is gone.
    fn selection_is_empty(&self) -> bool {
        let (Some(selection), Some(surface)) = (&self.selection, &self.live.surface) else {
            return true;
        };
        !self.live.surface_ready()
            || selection
                .rows(surface, self.cell_width, self.config.terminal.line_height())
                .next()
                .is_none()
    }

    /// The surface a press at `position` may select from, if the terminal area
    /// is showing one. Hit testing reads the live surface, never the frame a
    /// gap may still be presenting.
    fn selectable_surface(
        &self,
        position: Point<Pixels>,
    ) -> Option<&herdr_client::protocol::PaneSurfaceFrame> {
        if self.menu.page.is_some()
            || !self.live.surface_ready()
            || !self.bounds.contains(&position)
        {
            return None;
        }
        self.live.surface.as_deref()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
pub(super) mod tests;
