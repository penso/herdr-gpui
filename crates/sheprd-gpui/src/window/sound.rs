//! The window's share of the app's one sound worker.

use super::SheprdWindow;
use crate::sound::Service;
use std::rc::Rc;

impl SheprdWindow {
    /// Takes the app's shared sound worker and plays it at this window's
    /// volume. The volume is otherwise only set when a config load lands, never
    /// on a timer: a window whose reloads are held back, as while its theme
    /// picker is open, would keep restoring a stale volume over a newer one.
    pub(crate) fn use_sound(&mut self, sound: Rc<Service>) {
        sound.set_volume(self.config.sound.volume);
        self.sound = sound;
    }
}
