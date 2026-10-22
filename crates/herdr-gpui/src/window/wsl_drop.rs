//! Files dropped or pasted into a WSL distribution's pane paste as that
//! distribution's own paths for them: `/mnt/c/…` for a drive, or `/home/…`
//! for a file inside it, mapped by its `wslpath` on a background worker.
//! Nothing is copied, since a distribution already reads this machine's
//! drives, so a file of any size is ready at once and nothing is left behind.
//!
//! Mapping takes a `wsl.exe` round trip, so keys typed meanwhile are held
//! (see `pending_input`) and sent after the paths, as they were typed, or
//! discarded when the paths are not pasted. Paths
//! that arrive while a snapshot waits for its surface wait with those keys,
//! and are delivered once the same pane takes input again. One drop maps at
//! a time.

use super::{HerdrWindow, file_drop::quote_paths, pending_input::Delivery};
use crate::{Error, Result, connection::ConnectionBridge, terminal::InputTarget};
use gpui::Context;
use herdr_client::protocol::ClientPaneInputEvent;
use std::{
    path::PathBuf,
    sync::{Arc, atomic::AtomicBool},
    time::{Duration, Instant},
};

/// A snapshot's surface normally follows within a frame; mapped paths wait
/// this long for it before they are refused, so held keys cannot stall.
const GAP_LIMIT: Duration = Duration::from_secs(5);

/// A drop whose paths are being mapped, and where they were meant to go.
pub(crate) struct WslPaste {
    /// Tells this drop's answer from a later drop's once a reconnect has
    /// forgotten this one.
    token: Arc<()>,
    endpoint: String,
    generation: u64,
    epoch: u64,
    boot: String,
    target: InputTarget,
    /// The distribution's paths, once mapped, and when, until the pane
    /// takes them.
    mapped: Option<(Vec<String>, Instant)>,
}

impl HerdrWindow {
    pub(super) fn paste_wsl_files(
        &mut self,
        target: InputTarget,
        distro: String,
        paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Result<()> {
        if self.wsl_paste.is_some() {
            return Err(Error::WslPasteBusy);
        }
        // Records where held keys go before any are held.
        self.flush_pending_input(cx);
        let endpoint = &self.endpoints[self.selected_endpoint];
        let snapshot = self.live.snapshot.as_ref().ok_or(Error::NoSnapshot)?;
        let token = Arc::new(());
        let answer = token.clone();
        self.wsl_paste = Some(WslPaste {
            token,
            endpoint: endpoint.id.clone(),
            generation: endpoint.generation,
            epoch: self.selection_epoch,
            boot: snapshot.boot_id.clone(),
            target,
            mapped: None,
        });
        // Mapping is instant once the distribution runs; starting a stopped
        // one is bounded by herdr-client's idle limit, so nothing cancels it.
        let work = cx.background_executor().spawn(async move {
            herdr_client::wsl_paths(&distro, &paths, &AtomicBool::new(false))
        });
        cx.spawn(async move |this, cx| {
            let mapped = work.await;
            let _ = this.update(cx, |this, cx| this.finish_wsl_paste(&answer, mapped, cx));
        })
        .detach();
        Ok(())
    }

    fn finish_wsl_paste(
        &mut self,
        answer: &Arc<()>,
        mapped: herdr_client::Result<Vec<String>>,
        cx: &mut Context<Self>,
    ) {
        // A reconnect or device switch already forgot this drop.
        if !self
            .wsl_paste
            .as_ref()
            .is_some_and(|paste| Arc::ptr_eq(&paste.token, answer))
        {
            return;
        }
        match mapped {
            Ok(mapped) => {
                if let Some(paste) = &mut self.wsl_paste {
                    paste.mapped = Some((mapped, Instant::now()));
                }
            }
            Err(error) => {
                self.wsl_paste = None;
                self.local_error = Some(format!("Files not pasted: {}", Error::from(error)));
                // Keys typed after the drop never run without its paths.
                self.discard_all_held(Delivery::Refused, cx);
                cx.notify();
            }
        }
        // Delivers the paths and then the keys typed while they were mapped,
        // or leaves both waiting for the surface.
        self.flush_pending_input(cx);
    }

    /// Sends a mapped drop's paths, or refuses them with a reason. False
    /// while they cannot go yet: still being mapped, or a snapshot waiting
    /// for its surface on the same connection (for up to `GAP_LIMIT`), which
    /// polling retries.
    pub(super) fn deliver_wsl_paste(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(paste) = &self.wsl_paste else {
            return true;
        };
        let Some((_, mapped_at)) = &paste.mapped else {
            return false;
        };
        let endpoint = &self.endpoints[self.selected_endpoint];
        let same_connection = paste.epoch == self.selection_epoch
            && paste.generation == endpoint.generation
            && paste.endpoint == endpoint.id
            && self
                .live
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.boot_id == paste.boot);
        if same_connection
            && !self.disconnected()
            && self.menu.page.is_none()
            && !self.input_ready()
            && mapped_at.elapsed() < GAP_LIMIT
        {
            return false;
        }
        let Some(mut paste) = self.wsl_paste.take() else {
            return true;
        };
        let Some((mapped, _)) = paste.mapped.take() else {
            return true;
        };
        if let Err(error) = self.send_wsl_paths(&paste, mapped) {
            self.local_error = Some(format!("Files not pasted: {error}"));
            self.discard_all_held(Delivery::Refused, cx);
            cx.notify();
        }
        true
    }

    fn send_wsl_paths(&self, paste: &WslPaste, mapped: Vec<String>) -> Result<()> {
        let endpoint = &self.endpoints[self.selected_endpoint];
        // The window moved on to another device or connection, or the pane
        // or popup the files were dropped on no longer takes input.
        let current = paste.epoch == self.selection_epoch
            && paste.generation == endpoint.generation
            && paste.endpoint == endpoint.id
            && self.menu.page.is_none()
            && self.input_ready()
            && self
                .live
                .snapshot
                .as_ref()
                .is_some_and(|snapshot| snapshot.boot_id == paste.boot)
            && self.input_target_shown(&paste.target);
        if !current {
            return Err(Error::WslPasteTargetGone);
        }
        let text = quote_paths(&mapped.into_iter().map(PathBuf::from).collect::<Vec<_>>())?;
        let handle = endpoint
            .connection
            .handle
            .as_ref()
            .ok_or(Error::NotConnected)?;
        ConnectionBridge::send_input(
            handle,
            &paste.boot,
            &paste.target,
            ClientPaneInputEvent::Paste(text),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
