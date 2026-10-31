//! What the emulator keeps from alacritty's events. `Term` reports through a
//! shared `&self` listener, so the inbox sits behind a mutex the emulator
//! drains after every feed; it holds at most one title and a bell count, so
//! nothing in it grows with input.

use alacritty_terminal::event::{Event, EventListener};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

#[derive(Debug, Default)]
pub(crate) struct Inbox {
    /// The latest title change: `Some(None)` is a reset to the default.
    pub(crate) title: Option<Option<String>>,
    pub(crate) bells: u32,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct Listener(Arc<Mutex<Inbox>>);

impl Listener {
    pub(crate) fn take(&self) -> Inbox {
        std::mem::take(&mut *self.lock())
    }

    fn lock(&self) -> MutexGuard<'_, Inbox> {
        // The inbox holds plain values, so a poisoned lock is still consistent.
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        match event {
            Event::Bell => {
                let mut inbox = self.lock();
                inbox.bells = inbox.bells.saturating_add(1);
            }
            Event::Title(title) => self.lock().title = Some(Some(title)),
            Event::ResetTitle => self.lock().title = Some(None),
            // herdr-core answers terminal queries itself, and the emulator
            // never touches the clipboard or reports sizes and colors.
            Event::ClipboardLoad(..)
            | Event::ClipboardStore(..)
            | Event::ColorRequest(..)
            | Event::PtyWrite(_)
            | Event::TextAreaSizeRequest(_)
            | Event::ChildExit(_)
            | Event::CursorBlinkingChange
            | Event::Exit
            | Event::MouseCursorDirty
            | Event::Wakeup => {}
        }
    }
}
