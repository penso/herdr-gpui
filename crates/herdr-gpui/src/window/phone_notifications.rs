//! Hands notices the shared policy cleared for the phone to the app-wide
//! [`Dispatcher`], which rate-limits them and sends off the UI thread.

use super::HerdrWindow;
use crate::notifications::phone::{Dispatcher, Message};
use gpui::Context;
use std::time::Instant;

impl HerdrWindow {
    /// Runs from the poll loop only, after the notification tick. Without
    /// the dispatcher, which only normal launches install, it still takes
    /// the notices so phone-only ones do not linger.
    pub(crate) fn post_phone_notifications(&mut self, cx: &mut Context<Self>) {
        let posts =
            crate::notifications::take_phone(&mut self.endpoints, self.config.notifications.phone);
        if posts.is_empty() || !cx.has_global::<Dispatcher>() {
            return;
        }
        let Ok(service) = self.config.phone.service() else {
            return;
        };
        let now = Instant::now();
        for post in posts {
            let label = (self.endpoints.len() > 1)
                .then(|| self.endpoints.get(post.endpoint))
                .flatten()
                .map(|endpoint| crate::notifications::safe_text(&endpoint.label, 40));
            let dispatcher = cx.global_mut::<Dispatcher>();
            if !dispatcher.admit(&post.tag, now) {
                continue;
            }
            let message = Message::new(
                post.event,
                &post.title,
                post.body.as_deref(),
                label.as_deref(),
            );
            dispatcher.enqueue(service.clone(), message);
        }
    }
}
