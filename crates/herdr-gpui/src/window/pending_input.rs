//! Input made while a newer snapshot waits for its matching surface.
//!
//! The daemon publishes a snapshot before the surface of the same projection
//! revision, and an agent's title spinner bumps that revision several times a
//! second. Without a coherent surface the popup target is unknown, so input
//! waits here instead of being discarded, and leaves only for the target that
//! was focused when the gap opened. Keys, pastes, and file drops share one
//! queue, so they arrive in the order the user made them.

use super::{Flash, HerdrWindow};
use crate::{connection::ConnectionBridge, terminal::InputTarget};
use gpui::{Context, Pixels, Point};
use herdr_client::protocol::ClientPaneInputEvent;
use std::{collections::VecDeque, path::PathBuf};

/// Gaps normally close within a render tick; this bounds a stalled one.
const MAX_PENDING_EVENTS: usize = 256;

/// Held pastes and drops, matching the cap on concurrent image preparations
/// they may each start once delivered.
const MAX_PENDING_ACTIONS: usize = 4;

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
}

impl PendingInput {
    // Only the Unix socket lifecycle tests inspect the queue.
    #[cfg(all(test, unix))]
    pub(crate) fn len(&self) -> usize {
        self.events.len()
    }

    fn actions(&self) -> usize {
        self.events
            .iter()
            .filter(|held| !matches!(held, Held::Event(_)))
            .count()
    }
}

impl HerdrWindow {
    pub(crate) fn send(&mut self, event: ClientPaneInputEvent, cx: &mut Context<Self>) {
        if self.menu.page.is_some() || self.mouse_focus_pending() {
            return;
        }
        if !self.input_ready() {
            if self.disconnected() {
                // Nothing typed into a lost connection is replayed into the
                // next one: the user sees that it went nowhere.
                self.show_flash(Flash::warning("Not connected: input not sent"), cx);
                return;
            }
            // Typing across a projection gap is use of the window too.
            self.note_input();
            self.hold_input(event, cx);
            return;
        }
        if !self.flush_pending_input(cx) {
            return;
        }
        let Some(target) = &self.pending_input.target else {
            return;
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
        }
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
            self.replay_paste(image_only, fallback, cx);
        } else if self.disconnected() {
            self.show_flash(Flash::warning("Not connected: input not sent"), cx);
        } else {
            self.hold(
                Held::Paste {
                    image_only,
                    fallback,
                },
                cx,
            );
        }
    }

    /// Paste what the clipboard holds into the focused pane or popup. Only
    /// the background reader touches the native clipboard, except on Linux,
    /// where GPUI has no text-only clipboard API and its native ordinary
    /// paste needs no helper executable.
    fn replay_paste(
        &mut self,
        image_only: bool,
        fallback: Option<ClientPaneInputEvent>,
        cx: &mut Context<Self>,
    ) {
        if image_only {
            self.paste_native_clipboard(true, fallback, cx);
        } else if self.accepts_clipboard_images() && !cfg!(target_os = "linux") {
            self.paste_native_clipboard(false, None, cx);
        } else if let Some(item) = cx.read_from_clipboard() {
            self.paste_terminal_clipboard(item, false, cx);
        }
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

    /// Whether the selected endpoint is down or reconnecting after a drop.
    fn disconnected(&self) -> bool {
        !self.live.status.is_connected()
            || self.endpoints[self.selected_endpoint].outage().is_some()
    }

    fn hold_input(&mut self, event: ClientPaneInputEvent, cx: &mut Context<Self>) {
        self.hold(Held::Event(event), cx);
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
            self.local_error = Some("Input not sent: the terminal stopped updating".into());
            cx.notify();
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
        if self.pending_input.events.is_empty() {
            return true;
        }
        // Taken whole: a replayed paste sends through `send`, which flushes
        // again and must not deliver the entries after it first.
        let held = std::mem::take(&mut self.pending_input.events);
        // An unchanged target also means an unchanged boot, so a drop's own
        // pane is still the one it was dropped on.
        let target = self
            .pending_input
            .target
            .clone()
            .filter(|target| previous.as_ref() == Some(target));
        let Some(target) = target.filter(|_| {
            self.endpoints[self.selected_endpoint]
                .connection
                .handle
                .is_some()
        }) else {
            self.local_error = Some("Input not sent: focus changed before it arrived".into());
            cx.notify();
            return true;
        };
        let mut held = held.into_iter();
        while let Some(entry) = held.next() {
            match entry {
                Held::Event(event) => {
                    if !self.deliver_held_event(&target, event, held.len(), cx) {
                        break;
                    }
                }
                Held::Paste {
                    image_only,
                    fallback,
                } => self.replay_paste(image_only, fallback, cx),
                Held::Drop { at, paths } => self.deliver_held_drop(at, &paths, cx),
            }
        }
        true
    }

    /// Send one held key or text event; false, with the error and the
    /// `discarded` entries after it reported, when it could not be sent.
    fn deliver_held_event(
        &mut self,
        target: &Target,
        event: ClientPaneInputEvent,
        discarded: usize,
        cx: &mut Context<Self>,
    ) -> bool {
        let error = match &self.endpoints[self.selected_endpoint].connection.handle {
            Some(handle) => {
                match ConnectionBridge::send_input(handle, &target.boot, &target.input, event) {
                    Ok(()) => return true,
                    Err(error) => error.to_string(),
                }
            }
            None => crate::Error::NotConnected.to_string(),
        };
        let note = match discarded {
            0 => String::new(),
            1 => " (1 held input discarded)".into(),
            n => format!(" ({n} held inputs discarded)"),
        };
        // Display text only: the error's cause is not propagated further.
        self.local_error = Some(format!("Input not sent: {error}{note}"));
        cx.notify();
        false
    }

    /// A reconnect, detach, or endpoint switch starts with no target and
    /// nothing held for the old one.
    pub(crate) fn clear_pending_input(&mut self) {
        self.pending_input = PendingInput::default();
    }
}
