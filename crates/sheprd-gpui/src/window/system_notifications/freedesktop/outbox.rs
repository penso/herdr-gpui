//! One in-flight Notify and bounded, latest-per-tag pending posts. Reading
//! replaces_id at dispatch time makes even overlapping first posts replace
//! their predecessor, rather than creating a banner for every event.

use super::{Error, Registry, Session};
use gpui::{App, SystemNotification};
use std::{collections::VecDeque, future::Future};

#[derive(Default)]
pub(in super::super) struct Outbox {
    pending: VecDeque<SystemNotification>,
    running: bool,
}

impl Outbox {
    /// Whether the caller must start the single delivery task.
    fn enqueue(&mut self, notification: SystemNotification) -> bool {
        self.pending.retain(|old| old.tag != notification.tag);
        if self.pending.len() == super::super::REGISTRY_LIMIT {
            self.pending.pop_front();
        }
        self.pending.push_back(notification);
        !std::mem::replace(&mut self.running, true)
    }

    fn next(&mut self) -> Option<SystemNotification> {
        let notification = self.pending.pop_front();
        self.running = notification.is_some();
        notification
    }
}

pub(in super::super) fn post(session: Session, notification: SystemNotification, cx: &mut App) {
    post_with(notification, cx, move |notification, replaces| {
        let session = session.clone();
        async move { super::show(&session, &notification, replaces).await }
    });
}

fn post_with<F>(
    notification: SystemNotification,
    cx: &mut App,
    mut send: impl FnMut(SystemNotification, u32) -> F + 'static,
) where
    F: Future<Output = Result<u32, Error>> + 'static,
{
    if !cx.global_mut::<Registry>().outbox.enqueue(notification) {
        return;
    }
    cx.spawn(async move |cx| {
        loop {
            let next = cx.update(|cx| {
                let registry = cx.global_mut::<Registry>();
                while let Some(notification) = registry.outbox.next() {
                    // An evicted target must not post after its click routing is gone.
                    if let Some(posted) = registry.find(&notification.tag) {
                        let replaces = posted.bus_id.unwrap_or_default();
                        return Some((notification, replaces));
                    }
                }
                None
            });
            let Some((notification, replaces)) = next else {
                break;
            };
            let tag = notification.tag.clone();
            match send(notification, replaces).await {
                Ok(bus_id) => cx.update(|cx| {
                    cx.global_mut::<Registry>().set_bus_id(&tag, bus_id);
                }),
                // Keep the previous id on failure, so the next post can replace it.
                Err(error) => tracing::warn!(error = ?error, "System notification failed"),
            }
        }
    })
    .detach();
}

#[cfg(test)]
mod tests;
