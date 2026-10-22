//! Input held while an input-changing projection waits for its surface.
//!
//! Metadata-only updates retain a coherent displayed pair and send input
//! immediately. Other projection gaps may temporarily remove that pair, so
//! held input leaves only for the target that was focused when the gap opened.
//! Keys, pastes, and file drops share one queue, so they arrive in the order
//! the user made them. A WSL drop whose paths are still being mapped holds
//! input the same way, so keys typed or held after it land after it, and are
//! discarded if its paths are not pasted.
//!
//! A drop that starts a remote file copy ends the batch, because its paths are
//! pasted only when the copy completes; so does a paste of files copied in
//! Explorer onto an SSH pane, which starts the same copy.
//!
//! A held paste or drop that reads the clipboard or prepares an image in the
//! background stops delivery until that work ends. The input after it, and
//! input made meanwhile, waits behind it: delivered in order once its paste is
//! queued, discarded when it fails, is cancelled, or becomes a file copy.
//!
//! A paste or drop refused because too many waited discards the input the gap
//! held after it; input made once the frame returned still follows.

use super::{Flash, HerdrWindow};
use crate::{connection::ConnectionBridge, terminal::InputTarget};
use gpui::{Context, Pixels, Point};
use herdr_client::protocol::ClientPaneInputEvent;
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Weak},
};

/// Gaps normally close within a render tick; this bounds a stalled one.
const MAX_PENDING_EVENTS: usize = 256;

/// Held pastes and drops, matching the cap on concurrent image preparations
/// they may each start once delivered.
const MAX_PENDING_ACTIONS: usize = 4;

/// Shown when input is refused because the hold queue is full.
const STOPPED: &str = "Input not sent: the terminal stopped updating";

/// What delivering one held input did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Delivery {
    /// Queued on the connection, or there was nothing to send.
    Queued,
    /// A slot is reserved on the connection while the clipboard is read or
    /// an image prepared in the background; whether it is filled is known
    /// only when that work ends.
    Preparing,
    /// A remote file copy started; its paths are pasted when it completes.
    Copying,
    /// Refused, with the reason shown.
    Refused,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Target {
    boot: String,
    input: InputTarget,
}

/// One user action waiting for the gap to close.
enum Held {
    /// A key or text event, sent as it was made.
    Event(ClientPaneInputEvent),
    /// A paste, replayed against the clipboard as it is on delivery: nothing
    /// is read from the clipboard while it waits.
    Paste {
        image_only: bool,
        fallback: Option<ClientPaneInputEvent>,
    },
    /// Files dropped on the panes.
    Drop { at: DropAt, paths: Vec<PathBuf> },
    /// Where a paste or drop was refused because too many waited: the input
    /// the gap held after it is discarded rather than sent without it.
    Refused,
    /// Where the frame returned while held input waited on a background
    /// paste: what follows was made after the gap, so a refusal mark ahead
    /// does not discard it.
    FrameReturned,
}

/// Where held files go.
pub(super) enum DropAt {
    /// The pane or popup under the pointer in the frame the user saw.
    Pane(InputTarget),
    /// The drop point, when there was no frame to find a pane in: the frame
    /// that closes the gap decides.
    Point(Point<Pixels>),
}

#[derive(Default)]
pub(crate) struct PendingInput {
    /// Where input went at the last coherent projection.
    target: Option<Target>,
    events: VecDeque<Held>,
    /// The background preparation a held paste or drop started: input held
    /// after it waits for its outcome. A preparation dropped without one
    /// leaves this dangling, and the input behind it is discarded.
    behind: Option<Weak<()>>,
}

impl PendingInput {
    /// Held inputs and refusal marks.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.events
            .iter()
            .filter(|held| !matches!(held, Held::FrameReturned))
            .count()
    }

    fn actions(&self) -> usize {
        self.events
            .iter()
            .filter(|held| matches!(held, Held::Paste { .. } | Held::Drop { .. }))
            .count()
    }
}

impl HerdrWindow {
    /// Send `event` to the focused pane or popup, or hold it across a
    /// projection gap. False when it was neither sent nor held and the window
    /// said why; input ignored while a menu or a focus click owns the window,
    /// or outside a projection gap, returns true.
    pub(crate) fn send(&mut self, event: ClientPaneInputEvent, cx: &mut Context<Self>) -> bool {
        if self.menu.page.is_some() || self.mouse_focus_pending() {
            return true;
        }
        // Keys typed while a WSL drop's paths are mapped follow those paths.
        if !self.input_ready() || self.wsl_paste.is_some() {
            if self.disconnected() {
                // Nothing typed into a lost connection is replayed into the
                // next one: the user sees that it went nowhere.
                self.show_flash(Flash::warning("Not connected: input not sent"), cx);
                return false;
            }
            // Typing across a projection gap is use of the window too.
            self.note_input();
            // Outside a gap the input is ignored as before, with nothing shown.
            return self.hold(Held::Event(event), cx) || !self.in_projection_gap();
        }
        if !self.flush_pending_input(cx) {
            return true;
        }
        if self.waits_behind_preparation() {
            return self.hold_after_gap(Held::Event(event), cx) || !self.in_projection_gap();
        }
        let Some(target) = &self.pending_input.target else {
            return true;
        };
        // `target` borrows the window, so the field is set directly.
        self.last_input = Some(std::time::Instant::now());
        let handle = &self.endpoints[self.selected_endpoint].connection.handle;
        if let Some(handle) = handle
            && let Err(error) =
                ConnectionBridge::send_input(handle, &target.boot, &target.input, event)
        {
            self.local_error = Some(format!("Input not sent: {error}"));
            cx.notify();
            return false;
        }
        true
    }

    /// Paste the clipboard now, or hold the paste across a projection gap as
    /// typed keys are held. `image_only` with a `fallback` is Ctrl-V on a
    /// remote pane: an image is bridged, anything else sends the key.
    pub(super) fn paste_or_hold(
        &mut self,
        image_only: bool,
        fallback: Option<ClientPaneInputEvent>,
        cx: &mut Context<Self>,
    ) {
        if self.menu.page.is_some() {
            return;
        }
        // A paste is use even before the frame is ready, so the window takes
        // back a frame another client sized.
        self.note_input();
        if self.mouse_focus_pending() {
            return;
        }
        if self.input_ready() {
            // Input held before the frame returned goes first.
            self.flush_pending_input(cx);
            if !self.waits_behind_preparation() {
                self.replay_paste(image_only, fallback, cx);
                return;
            }
            let held = Held::Paste {
                image_only,
                fallback,
            };
            self.hold_after_gap(held, cx);
        } else if self.disconnected() {
            self.show_flash(Flash::warning("Not connected: input not sent"), cx);
        } else {
            let held = Held::Paste {
                image_only,
                fallback,
            };
            self.hold(held, cx);
        }
    }

    /// Paste what the clipboard holds into the focused pane or popup. On
    /// macOS only the background reader touches the native clipboard; Linux
    /// has no text-only clipboard API in GPUI and its native ordinary paste
    /// needs no helper executable, and Windows has no background reader.
    fn replay_paste(
        &mut self,
        image_only: bool,
        fallback: Option<ClientPaneInputEvent>,
        cx: &mut Context<Self>,
    ) -> Delivery {
        if image_only {
            self.paste_native_clipboard(true, fallback, cx)
        } else if cfg!(target_os = "macos") {
            self.paste_native_clipboard(false, None, cx)
        } else if let Some(item) = cx.read_from_clipboard() {
            self.paste_item(item, false, cx).unwrap_or(Delivery::Queued)
        } else {
            Delivery::Queued
        }
    }

    /// Why a paste cannot go out now, with the reason shown, or None.
    pub(super) fn refuse_paste(&mut self, cx: &mut Context<Self>) -> Option<Delivery> {
        let reason = if self.menu.page.is_some() {
            "Paste not sent: a menu is open"
        } else if self.mouse_focus_pending() {
            "Paste not sent: the window was taking focus"
        } else {
            return None;
        };
        self.local_error = Some(reason.into());
        cx.notify();
        Some(Delivery::Refused)
    }

    /// Hold dropped files across a projection gap. Returns whether they were
    /// held.
    pub(super) fn hold_drop(
        &mut self,
        at: DropAt,
        paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) -> bool {
        self.hold(Held::Drop { at, paths }, cx)
    }

    /// Hold files dropped once the frame returned, behind held input that
    /// waits on a background paste. Returns whether they were held.
    pub(super) fn hold_drop_after_gap(
        &mut self,
        target: InputTarget,
        paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) -> bool {
        let at = DropAt::Pane(target);
        self.hold_after_gap(Held::Drop { at, paths }, cx)
    }

    /// Hold input made once the frame returned behind held input that waits
    /// on a background paste, past the mark that ends what the gap held.
    fn hold_after_gap(&mut self, held: Held, cx: &mut Context<Self>) -> bool {
        let events = &mut self.pending_input.events;
        if !events
            .iter()
            .any(|held| matches!(held, Held::FrameReturned))
        {
            events.push_back(Held::FrameReturned);
        }
        self.hold(held, cx)
    }

    /// Whether the selected endpoint is down or reconnecting after a drop.
    pub(super) fn disconnected(&self) -> bool {
        !self.live.status.is_connected()
            || self.endpoints[self.selected_endpoint].outage().is_some()
    }

    /// Hold input only across a projection gap on an unchanged connection.
    /// Toasts, navigation, and activation choose a new target on purpose, so
    /// input during them is still ignored as before. Returns whether `held`
    /// was queued.
    fn hold(&mut self, held: Held, cx: &mut Context<Self>) -> bool {
        if !self.in_projection_gap() {
            return false;
        }
        let full = self.pending_input.events.len() >= MAX_PENDING_EVENTS
            || (!matches!(held, Held::Event(_))
                && self.pending_input.actions() >= MAX_PENDING_ACTIONS);
        if full {
            self.local_error = Some(STOPPED.into());
            cx.notify();
            // A refused paste or drop ends the batch where it was made.
            let marked = matches!(self.pending_input.events.back(), Some(Held::Refused));
            if !matches!(held, Held::Event(_))
                && !marked
                && self.pending_input.events.len() < MAX_PENDING_EVENTS
            {
                self.pending_input.events.push_back(Held::Refused);
            }
            return false;
        }
        self.pending_input.events.push_back(held);
        true
    }

    fn in_projection_gap(&self) -> bool {
        self.pending_toast.is_none()
            && self.pending_navigation.is_none()
            && self.live.status.is_connected()
            && !self.live.activation_pending()
            && self.endpoints[self.selected_endpoint]
                .connection
                .handle
                .is_some()
            && self
                .pending_input
                .target
                .as_ref()
                .zip(self.live.snapshot.as_ref())
                .is_some_and(|(target, snapshot)| target.boot == snapshot.boot_id)
    }

    /// Record the current target and deliver held input to it in order, or
    /// discard it if focus moved during the gap. Returns whether the
    /// projection is coherent, so the caller may send after held input.
    pub(crate) fn flush_pending_input(&mut self, cx: &mut Context<Self>) -> bool {
        // A WSL drop's paths go before keys typed after it.
        if self.wsl_paste.is_some() && !self.deliver_wsl_paste(cx) {
            return false;
        }
        if !self.input_ready() {
            return false;
        }
        let target = self
            .live
            .snapshot
            .as_ref()
            .zip(self.focused_input_target())
            .map(|(snapshot, input)| Target {
                boot: snapshot.boot_id.clone(),
                input,
            });
        let previous = std::mem::replace(&mut self.pending_input.target, target);
        // An unchanged target also means an unchanged boot, so a drop's own
        // pane is still the one it was dropped on.
        let unchanged = self
            .pending_input
            .target
            .as_ref()
            .is_some_and(|target| previous.as_ref() == Some(target))
            && self.endpoints[self.selected_endpoint]
                .connection
                .handle
                .is_some();
        if let Some(behind) = &self.pending_input.behind {
            if unchanged && behind.strong_count() > 0 {
                // The preparation's outcome releases or discards the input.
                return true;
            }
            self.pending_input.behind = None;
            if unchanged {
                self.discard_all_held(Delivery::Refused, cx);
                return true;
            }
        }
        if self.pending_input.events.is_empty() {
            return true;
        }
        // Taken whole: a replayed paste sends through `send`, which flushes
        // again and must not deliver the entries after it first.
        let held = std::mem::take(&mut self.pending_input.events);
        match self.pending_input.target.clone().filter(|_| unchanged) {
            Some(target) => self.deliver_held(&target, held, cx),
            None => {
                self.local_error = Some("Input not sent: focus changed before it arrived".into());
                cx.notify();
            }
        }
        true
    }

    /// Deliver `held` to `target` in order, until one entry does not go out
    /// whole: the input after it is discarded, or, behind a background
    /// preparation, held again for its outcome.
    fn deliver_held(&mut self, target: &Target, held: VecDeque<Held>, cx: &mut Context<Self>) {
        let mut held = held.into_iter();
        while let Some(entry) = held.next() {
            let delivery = match entry {
                Held::Event(event) => self.deliver_held_event(target, event, cx),
                Held::Paste {
                    image_only,
                    fallback,
                } => {
                    let shown = self.local_error.take();
                    let delivery = self.replay_paste(image_only, fallback, cx);
                    self.keep_shown_error(shown, delivery, held.len());
                    delivery
                }
                Held::Drop { at, paths } => {
                    let shown = self.local_error.take();
                    let delivery = self.deliver_held_drop(at, &paths, cx);
                    self.keep_shown_error(shown, delivery, held.len());
                    delivery
                }
                Held::Refused => {
                    // The refusal was shown when it was made; it is shown
                    // again only with the gap input discarded after it.
                    // Input made once the frame returned still follows.
                    let discarded = held
                        .by_ref()
                        .take_while(|held| !matches!(held, Held::FrameReturned))
                        .filter(|held| !matches!(held, Held::Refused))
                        .count();
                    if discarded > 0 {
                        self.local_error = Some(STOPPED.into());
                        self.discard_held(Delivery::Refused, discarded, cx);
                    }
                    Delivery::Queued
                }
                Held::FrameReturned => Delivery::Queued,
            };
            match delivery {
                Delivery::Queued if self.wsl_paste.is_none() => {}
                // A WSL drop queues its paths only once they are mapped, so
                // the input after it waits for them, as it does behind a
                // background preparation.
                Delivery::Queued | Delivery::Preparing => {
                    // Input held during this delivery joins the back.
                    let mut waiting: VecDeque<Held> = held.collect();
                    waiting.append(&mut self.pending_input.events);
                    self.pending_input.events = waiting;
                    if delivery == Delivery::Preparing {
                        self.pending_input.behind = self.newest_preparation();
                    }
                    return;
                }
                Delivery::Copying | Delivery::Refused => {
                    let remaining = held
                        .filter(|held| !matches!(held, Held::FrameReturned))
                        .count();
                    self.discard_held(delivery, remaining, cx);
                    return;
                }
            }
        }
    }

    /// Whether held input waits for a background preparation's outcome, so
    /// new input must wait behind it too.
    pub(super) fn waits_behind_preparation(&self) -> bool {
        self.pending_input.behind.is_some()
    }

    /// The preparation identified by `token` ended as `delivery`. Input held
    /// behind it is delivered once its paste is queued, keeps waiting behind
    /// a preparation it started in turn, and is otherwise discarded.
    pub(super) fn release_held_behind(
        &mut self,
        token: &Arc<()>,
        delivery: Delivery,
        cx: &mut Context<Self>,
    ) {
        let waiting = self
            .pending_input
            .behind
            .as_ref()
            .is_some_and(|behind| std::ptr::eq(behind.as_ptr(), Arc::as_ptr(token)));
        if !waiting {
            return;
        }
        self.pending_input.behind = None;
        match delivery {
            Delivery::Queued => {
                self.flush_pending_input(cx);
            }
            Delivery::Preparing => self.pending_input.behind = self.newest_preparation(),
            Delivery::Copying | Delivery::Refused => self.discard_all_held(delivery, cx),
        }
    }

    /// Drop everything held, after the reason shown for `delivery`.
    pub(super) fn discard_all_held(&mut self, delivery: Delivery, cx: &mut Context<Self>) {
        let remaining = self
            .pending_input
            .events
            .drain(..)
            .filter(|held| !matches!(held, Held::Refused | Held::FrameReturned))
            .count();
        self.discard_held(delivery, remaining, cx);
    }

    /// Put back the error shown before a held paste or drop that set none of
    /// its own, unless the discard note for the `remaining` input after it is
    /// about to be shown: the note never follows an older, unrelated error.
    fn keep_shown_error(&mut self, shown: Option<String>, delivery: Delivery, remaining: usize) {
        let delivered = matches!(delivery, Delivery::Queued | Delivery::Preparing);
        if self.local_error.is_none() && (delivered || remaining == 0) {
            self.local_error = shown;
        }
    }

    /// Send one held key or text event; refused, with the error shown, when
    /// it could not be sent.
    fn deliver_held_event(
        &mut self,
        target: &Target,
        event: ClientPaneInputEvent,
        cx: &mut Context<Self>,
    ) -> Delivery {
        let error = match &self.endpoints[self.selected_endpoint].connection.handle {
            Some(handle) => {
                match ConnectionBridge::send_input(handle, &target.boot, &target.input, event) {
                    Ok(()) => return Delivery::Queued,
                    Err(error) => error.to_string(),
                }
            }
            None => crate::Error::NotConnected.to_string(),
        };
        // Display text only: the error's cause is not propagated further.
        self.local_error = Some(format!("Input not sent: {error}"));
        cx.notify();
        Delivery::Refused
    }

    /// Report the `remaining` held inputs dropped after one that was refused
    /// or started a remote file copy, after the reason it did not go out.
    fn discard_held(&mut self, delivery: Delivery, remaining: usize, cx: &mut Context<Self>) {
        let note = match remaining {
            0 => return,
            1 => "(1 held input discarded)".to_owned(),
            n => format!("({n} held inputs discarded)"),
        };
        let reason = match delivery {
            Delivery::Copying => "Input not sent: the files are still copying".to_owned(),
            Delivery::Queued | Delivery::Preparing | Delivery::Refused => self
                .local_error
                .take()
                .unwrap_or_else(|| "Input not sent".to_owned()),
        };
        self.local_error = Some(format!("{reason} {note}"));
        cx.notify();
    }

    /// A reconnect, detach, or endpoint switch starts with no target and
    /// nothing held for the old one.
    pub(crate) fn clear_pending_input(&mut self) {
        self.pending_input = PendingInput::default();
        // Its paths name files for the old connection's pane.
        self.wsl_paste = None;
    }
}

#[cfg(test)]
mod tests;
