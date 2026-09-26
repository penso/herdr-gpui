//! Keeping the window's view of the daemon current: resending geometry only
//! when it actually changed, renaming the OS window after the focused space,
//! and reporting focus to the authoritative inbox as well as the wire.

use super::HerdrWindow;
use crate::{WINDOW_TITLE, sidebar};
use gpui::Window;
use std::time::{Duration, Instant};

/// A drag or the full screen animation passes through many sizes, and each
/// resize makes every pane's program redraw, agents re-rendering whole
/// transcripts. Only the size the window settles on is sent.
pub(crate) const RESIZE_SETTLE: Duration = Duration::from_millis(150);

impl HerdrWindow {
    pub(crate) fn resize(&mut self) {
        self.resize_at(Instant::now());
    }

    pub(crate) fn resize_at(&mut self, now: Instant) {
        if self.last_queued_options == Some(self.options) {
            self.pending_resize = None;
            return;
        }
        // The first geometry goes out at once; later changes wait to settle.
        if self.last_queued_options.is_some() {
            match self.pending_resize {
                Some((options, since)) if options == self.options => {
                    if now.duration_since(since) < RESIZE_SETTLE {
                        return;
                    }
                }
                _ => {
                    self.pending_resize = Some((self.options, now));
                    return;
                }
            }
        }
        if let (Some(handle), Some(snapshot)) = (
            &self.endpoints[self.selected_endpoint].connection.handle,
            &self.live.snapshot,
        ) {
            match handle.resize(&snapshot.boot_id, self.options) {
                Ok(()) => self.last_queued_options = Some(self.options),
                Err(error) => self.local_error = Some(format!("Resize: {error}")),
            }
        }
    }

    /// macOS lists every window in the Window menu by title. Windows onto the
    /// same daemon are told apart by the space each one is showing.
    pub(crate) fn sync_window_title(&mut self, window: &mut Window) {
        let title = self
            .live
            .snapshot
            .as_ref()
            .and_then(|snapshot| {
                let focused = snapshot.focused_workspace_id.as_deref()?;
                snapshot
                    .workspaces
                    .iter()
                    .find(|workspace| workspace.workspace_id == focused)
            })
            .map_or_else(
                || WINDOW_TITLE.to_owned(),
                |workspace| {
                    format!(
                        "{WINDOW_TITLE} \u{2014} {}",
                        sidebar::workspace_label(workspace, false)
                    )
                },
            );
        if self.title != title {
            window.set_window_title(&title);
            self.title = title;
        }
    }

    pub(crate) fn report_focus(&mut self) {
        // The first positive report acknowledges the daemon's active tab. Wait
        // until its surface is coherent, but not necessarily our size: focus
        // claims tab geometry, so waiting for that size can deadlock startup.
        // Do not flap focus on later frame gaps.
        let focused = self.active
            && self.endpoints[self.selected_endpoint].surface_requested()
            && (self.sent_focus == Some(true)
                || (self.pending_toast.is_none() && self.surface_activation_ready()));
        // Update the authoritative event inbox, not just the rendered clone.
        if let Ok(mut state) = self.endpoints[self.selected_endpoint]
            .connection
            .inbox
            .try_lock()
        {
            state.set_outer_focus(self.active && self.input_ready());
        }
        if self.sent_focus == Some(focused) {
            return;
        }
        if let (Some(handle), Some(snapshot)) = (
            &self.endpoints[self.selected_endpoint].connection.handle,
            &self.live.snapshot,
        ) && handle.set_focus(&snapshot.boot_id, focused).is_ok()
        {
            self.sent_focus = Some(focused);
        }
    }
}
