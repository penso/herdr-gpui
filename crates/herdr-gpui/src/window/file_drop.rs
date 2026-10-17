//! External paths paste locally; one supported image on SSH uses the daemon's
//! image bridge. A drop never submits the terminal command.

use super::{HerdrWindow, pending_input::DropAt};
use crate::{
    Error, Result,
    connection::ConnectionBridge,
    terminal::{InputTarget, escape_into, escaped_len, wheel_target},
};
use gpui::{Context, ExternalPaths, Pixels, Point, Window};
use herdr_client::protocol::ClientPaneInputEvent;
use std::path::PathBuf;

pub(super) const MAX_PATHS: usize = 256;
const MAX_PASTE_BYTES: usize = 64 * 1024;

impl HerdrWindow {
    pub(crate) fn drop_terminal_files(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // GPUI translates the platform Submit position into window mouse state
        // before invoking on_drop; the payload itself carries no coordinates.
        let position = window.mouse_position();
        // A drop over the panes is use even before the frame is ready, so the
        // window takes back a frame another client sized.
        if self.menu.page.is_none() && self.bounds.contains(&position) {
            self.note_input();
        }
        if self.input_ready() {
            let Some(target) = self.file_drop_target(position) else {
                return;
            };
            cx.stop_propagation();
            if self.paste_files(target, paths.paths(), cx) {
                window.focus(&self.focus, cx);
            }
            return;
        }
        // The files wait for a ready frame. Delivery has no window to focus,
        // so the window takes focus as the drop is held.
        if paths.paths().is_empty() {
            return;
        }
        let Some(at) = self.held_drop_at(position) else {
            return;
        };
        cx.stop_propagation();
        if self.hold_drop(at, paths.paths().to_vec(), cx) {
            window.focus(&self.focus, cx);
        }
    }

    /// Where a drop made before the frame is ready goes: the pane under the
    /// pointer in the frame the user sees, or, in a gap with no frame, the
    /// drop point for the frame that closes it.
    fn held_drop_at(&self, position: Point<Pixels>) -> Option<DropAt> {
        if self.live.surface.is_some() {
            return self.pane_under_drop(position).map(DropAt::Pane);
        }
        (self.menu.page.is_none()
            && self.live.status.is_connected()
            && self.bounds.contains(&position))
        .then_some(DropAt::Point(position))
    }

    /// Paste held files once the frame is ready; a drop point that lands on
    /// nothing in that frame is reported.
    pub(super) fn deliver_held_drop(
        &mut self,
        at: DropAt,
        paths: &[PathBuf],
        cx: &mut Context<Self>,
    ) {
        let target = match at {
            DropAt::Pane(target) => Some(target),
            DropAt::Point(position) => self.pane_under_drop(position),
        };
        if let Some(target) = target {
            self.paste_files(target, paths, cx);
        } else {
            self.local_error = Some("Files not pasted: nothing under the drop point".into());
            cx.notify();
        }
    }

    /// Paste external files into `target` the way a drop does: quoted local
    /// paths on a local pane, the image bridge for one image on a remote pane,
    /// and a file copy for anything else there. Files copied in Finder and
    /// pasted with Cmd-V take this path too. False when nothing was sent.
    pub(super) fn paste_files(
        &mut self,
        target: InputTarget,
        paths: &[PathBuf],
        cx: &mut Context<Self>,
    ) -> bool {
        if paths.is_empty() {
            return false;
        }
        // Local paths mean nothing on a cloud machine, and file copies
        // there would need the provider's own transfer; refuse rather than paste dead paths.
        #[cfg(feature = "cloud")]
        if matches!(
            self.endpoints[self.selected_endpoint].connection.target,
            herdr_client::ConnectTarget::Cloud { .. }
        ) {
            self.local_error =
                Some("Files not pasted: file drops are not supported on cloud devices yet.".into());
            cx.notify();
            return false;
        }
        // Where the pane runs decides local paths versus bridging, never its
        // readiness: a local path names nothing on another host. Readiness can
        // lapse while Cmd-V reads the clipboard (a resize, say), so a remote
        // pane that is not ready refuses the files instead.
        let remote = self.selected_is_remote();
        let result = quote_paths(paths).and_then(|text| {
            if remote {
                if !self.accepts_remote_images() {
                    return Err(Error::ConnectionNotReady);
                }
                if let [path] = paths
                    && let Some(source) = super::image_source::from_path(path)
                {
                    self.start_remote_image(target.clone(), source, Some(text), cx);
                } else {
                    self.start_file_transfer(target.clone(), paths.to_vec(), cx);
                }
                return Ok(());
            }
            let endpoint = &self.endpoints[self.selected_endpoint];
            let handle = endpoint
                .connection
                .handle
                .as_ref()
                .ok_or(Error::NotConnected)?;
            let snapshot = self.live.snapshot.as_ref().ok_or(Error::NoSnapshot)?;
            ConnectionBridge::send_input(
                handle,
                &snapshot.boot_id,
                &target,
                ClientPaneInputEvent::Paste(text),
            )?;
            Ok(())
        });
        match result {
            Ok(()) => {
                self.note_input();
                true
            }
            Err(error) => {
                self.local_error = Some(format!("Files not pasted: {error}"));
                cx.notify();
                false
            }
        }
    }

    fn file_drop_target(&self, position: Point<Pixels>) -> Option<InputTarget> {
        if !self.input_ready() {
            return None;
        }
        self.pane_under_drop(position)
    }

    /// The pane or popup under `position` in the current frame, ready for
    /// input or not.
    fn pane_under_drop(&self, position: Point<Pixels>) -> Option<InputTarget> {
        if self.menu.page.is_some()
            || !self.live.status.is_connected()
            || !self.bounds.contains(&position)
        {
            return None;
        }
        wheel_target(
            self.live.surface.as_deref()?,
            f32::from(position.x - self.bounds.origin.x),
            f32::from(position.y - self.bounds.origin.y),
            self.cell_width,
            self.config.terminal.line_height(),
        )
        .map(|hit| hit.target)
    }
}

/// POSIX shell words, separated by spaces without a trailing newline. Like
/// native macOS terminals, a path stays bare and readable, with a backslash
/// before each character the shell would otherwise interpret. Validate the
/// whole drop before allocating its output so failures never paste a prefix.
pub(super) fn quote_paths(paths: &[PathBuf]) -> Result<String> {
    if paths.len() > MAX_PATHS {
        return Err(Error::FileDropSize);
    }
    let mut bytes = paths.len().saturating_sub(1);
    for path in paths {
        // Bound UTF-8 validation and escape counting as well as output allocation.
        if path.as_os_str().len() > MAX_PASTE_BYTES {
            return Err(Error::FileDropSize);
        }
        let text = path.to_str().ok_or(Error::FileDropEncoding)?;
        if text.is_empty() {
            return Err(Error::FileDropEmptyPath);
        }
        if text.chars().any(char::is_control) {
            return Err(Error::FileDropControl);
        }
        bytes += escaped_len(text);
        if bytes > MAX_PASTE_BYTES {
            return Err(Error::FileDropSize);
        }
    }
    let mut quoted = String::with_capacity(bytes);
    for path in paths {
        if !quoted.is_empty() {
            quoted.push(' ');
        }
        escape_into(path.to_str().ok_or(Error::FileDropEncoding)?, &mut quoted);
    }
    Ok(quoted)
}

#[cfg(test)]
mod tests;
