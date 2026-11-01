//! A terminal view's size: while a phone follows a terminal at its own
//! size, the view holds a direct attach that makes Herdr size the pane's
//! terminal to it. Dropping the view drops the attach, and Herdr sizes the
//! terminal back to its layout.

use super::super::wire::TermSize;
use crate::web::Shared;
use herdr_client::{TerminalAttach, protocol::AttachScrollDirection, terminal_attach};

/// The attach a terminal view holds, and the size it holds it at.
#[derive(Default)]
pub(super) struct Sizing {
    held: Option<Held>,
    size: Option<TermSize>,
}

impl Sizing {
    /// Holds `pane`'s terminal at `wanted`, or lets it go when `None`.
    /// Returns whether the size changed. A failed attach leaves the
    /// terminal at its layout size; the view goes on regardless.
    pub(super) async fn apply(
        &mut self,
        state: &Shared,
        pane: &str,
        wanted: Option<TermSize>,
    ) -> bool {
        if wanted == self.size {
            return false;
        }
        self.size = wanted;
        let Some(size) = wanted else {
            self.held = None;
            return true;
        };
        if let Some(held) = &self.held
            && held.attach.as_ref().is_some_and(|attach| {
                attach.is_attached() && attach.resize(size.cols, size.rows).is_ok()
            })
        {
            return true;
        }
        self.held = None;
        match state.hub.attach_terminal(pane, size.cols, size.rows).await {
            Ok(attach) => {
                tracing::debug!(pane, size.cols, size.rows, "terminal sized to the phone");
                self.held = Some(Held {
                    attach: Some(attach),
                });
            }
            Err(error) => {
                tracing::info!(pane, %error, "terminal stays at its layout size");
            }
        }
        true
    }

    /// Scrolls the held terminal `lines`, up toward its history when
    /// positive. False when no attach holds it to take the scroll.
    pub(super) fn scroll(&self, lines: i32) -> bool {
        let Some(attach) = self.held.as_ref().and_then(|held| held.attach.as_ref()) else {
            return false;
        };
        let direction = if lines > 0 {
            AttachScrollDirection::Up
        } else {
            AttachScrollDirection::Down
        };
        let lines = u16::try_from(lines.unsigned_abs()).unwrap_or(u16::MAX);
        match attach.scroll(direction, lines) {
            // Herdr is behind on scrolls: this one is dropped, not refused.
            Ok(()) | Err(terminal_attach::Error::Busy) => true,
            Err(error) => {
                tracing::debug!(%error, "terminal scroll not sent");
                false
            }
        }
    }
}

/// An attach dropped off the async threads: dropping one waits briefly
/// for its worker to send `Detach`.
struct Held {
    attach: Option<TerminalAttach>,
}

impl Drop for Held {
    fn drop(&mut self) {
        let Some(attach) = self.attach.take() else {
            return;
        };
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                runtime.spawn_blocking(move || drop(attach));
            }
            Err(_) => drop(attach),
        }
    }
}
