use super::HerdrWindow;
use crate::notifications::{Notice, VISIBLE_LIMIT};
use gpui::{prelude::*, *};
use herdr_client::protocol::{SemanticNotification, SemanticNotificationKind, ToastHerdrPosition};
use std::{sync::TryLockError, task::Poll, time::Instant};

mod badge;
mod card;
pub(crate) mod motion;

use card::Look;
use motion::{Drawn, Key};

impl HerdrWindow {
    pub(crate) fn click_toast(
        &mut self,
        endpoint_id: &str,
        generation: u64,
        inbox: &std::sync::Arc<std::sync::Mutex<crate::state::LiveState>>,
        id: u64,
        cx: &mut Context<Self>,
    ) {
        if self.menu.page.is_some() || self.toasts_hidden {
            return;
        }
        let Some(index) = self.endpoints.iter().position(|e| {
            e.id == endpoint_id
                && e.generation == generation
                && std::sync::Arc::ptr_eq(&e.connection.inbox, inbox)
        }) else {
            return;
        };
        self.open_notice(index, id, cx);
    }

    /// Selects the notice's host and navigates once its inbox validates the
    /// target, for a toast click and an OS notification click alike.
    pub(super) fn open_notice(&mut self, index: usize, id: u64, cx: &mut Context<Self>) {
        let target = match self.toast_target(index, id) {
            Poll::Ready(target) => target,
            Poll::Pending => {
                // The displayed target can start a handoff, but only a fresh
                // inbox validation may queue navigation after contention ends.
                let endpoint = &self.endpoints[index];
                endpoint.live.latest_snapshot().and_then(|snapshot| {
                    endpoint
                        .toasts
                        .entries
                        .iter()
                        .find(|(entry, _)| *entry == id)
                        .and_then(|(_, notice)| notice.target(snapshot))
                        .map(|target| (&target).into())
                })
            }
        };
        let Some(target) = target else {
            return;
        };
        let endpoint_id = self.endpoints[index].id.clone();
        if !self.select_endpoint(&endpoint_id, cx) {
            return;
        }
        self.pending_navigation = Some(target);
        self.pending_toast = Some(id);
        self.navigate_toast(id, cx);
        cx.notify();
    }

    pub(crate) fn toast_target(
        &self,
        index: usize,
        id: u64,
    ) -> Poll<Option<crate::navigation::OwnedNavigationTarget>> {
        let endpoint = &self.endpoints[index];
        if !endpoint.enabled || endpoint.connection.handle.is_none() {
            return Poll::Ready(None);
        }
        let Some((_, notice)) = endpoint
            .toasts
            .entries
            .iter()
            .find(|(entry, _)| *entry == id)
        else {
            return Poll::Ready(None);
        };
        let accepted = index == self.selected_endpoint && self.pending_toast == Some(id);
        // A posted OS notification has no in-app lifetime; its click is
        // fenced by the boot, inbox, and loss checks below instead.
        if !notice.posted && (!notice.visible || (!accepted && notice.expires <= Instant::now())) {
            return Poll::Ready(None);
        }
        let state = match endpoint.connection.inbox.try_lock() {
            Ok(state) => state,
            Err(TryLockError::WouldBlock) => return Poll::Pending,
            Err(TryLockError::Poisoned(_)) => return Poll::Ready(None),
        };
        if !state.status.is_connected()
            || state.notifications_lost
            || notice.pane_id.as_ref().is_some_and(|pane| {
                state
                    .notifications
                    .iter()
                    .any(|new| new.pane_id.as_ref() == Some(pane))
            })
        {
            return Poll::Ready(None);
        }
        Poll::Ready(
            state
                .latest_snapshot()
                .and_then(|snapshot| notice.target(snapshot))
                .map(|target| (&target).into()),
        )
    }

    pub(crate) fn navigate_toast(&mut self, id: u64, cx: &mut Context<Self>) {
        if self.menu.page.is_some() || !self.navigation_ready() {
            return;
        }
        let Poll::Ready(target) = self.toast_target(self.selected_endpoint, id) else {
            return;
        };
        self.pending_toast = None;
        self.pending_navigation = None;
        if let Some(target) = target
            && self.navigate(target.as_deref(), cx)
        {
            self.endpoints[self.selected_endpoint].toasts.dismiss(id);
        }
        cx.notify();
    }

    pub(crate) fn show_toast_preview(
        &mut self,
        kind: SemanticNotificationKind,
        cx: &mut Context<Self>,
    ) {
        let (title, body) = match kind {
            SemanticNotificationKind::NeedsAttention => (
                "Needs attention",
                "QA preview: an agent is waiting for your input.",
            ),
            SemanticNotificationKind::Finished => {
                ("Finished", "QA preview: an agent has completed its task.")
            }
            SemanticNotificationKind::UpdateInstalled => (
                "Update installed",
                "QA preview only. No update was installed.",
            ),
            SemanticNotificationKind::Custom => (
                "Custom notification",
                "QA preview: a custom notification message.",
            ),
        };
        let snapshot = self.endpoints[self.selected_endpoint].live.snapshot.clone();
        let target = snapshot.as_ref().filter(|_| {
            matches!(
                kind,
                SemanticNotificationKind::NeedsAttention | SemanticNotificationKind::Finished
            )
        });
        let mut notice = Notice::new(
            SemanticNotification {
                kind,
                title: title.into(),
                body: Some(body.into()),
                sound: None,
                agent: None,
                workspace_id: target.and_then(|s| s.focused_workspace_id.clone()),
                tab_id: target.and_then(|s| s.focused_tab_id.clone()),
                pane_id: target.and_then(|s| s.focused_pane_id.clone()),
                position: None,
            },
            Instant::now(),
        )
        // Previews keep their own text rather than the pane's context.
        .preview()
        .with_snapshot(snapshot.as_deref());
        // Each QA action immediately presents its own card, even offline.
        for endpoint in &mut self.endpoints {
            endpoint.toasts.entries.retain(|(_, n)| !n.visible);
        }
        notice.position = self.config.notifications.position;
        notice.promote(Instant::now());
        self.endpoints[self.selected_endpoint]
            .toasts
            .receive([notice]);
        self.tick_toasts(false, Instant::now());
        cx.notify();
    }

    pub(crate) fn tick_toasts(&mut self, hidden: bool, now: Instant) -> bool {
        crate::notifications::tick(
            &mut self.endpoints,
            self.selected_endpoint,
            self.config.notifications,
            hidden,
            self.active,
            self.pending_toast,
            now,
        )
    }

    pub(super) fn render_toasts(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let viewport = window.viewport_size();
        self.toasts_hidden = viewport.height < px(180.) || viewport.width < px(180.);
        self.tick_toasts(
            self.menu.page.is_some() || self.toasts_hidden,
            Instant::now(),
        );
        // Hidden toasts keep their motion where it was: they come back as
        // they left once the menu closes or the window grows.
        if self.menu.page.is_some() || self.toasts_hidden {
            return Vec::new();
        }
        let visible_limit =
            ((viewport.height.to_f64() as usize).saturating_sub(108) / 108).clamp(1, VISIBLE_LIMIT);
        let narrow = viewport.width < px(720.);
        let width = (viewport.width - px(24.)).max(px(0.)).min(px(340.));
        let visible: Vec<_> = self
            .endpoints
            .iter()
            .flat_map(|endpoint| {
                endpoint
                    .toasts
                    .entries
                    .iter()
                    .filter(|(_, notice)| notice.visible)
                    .map(move |(id, notice)| {
                        let navigable = endpoint
                            .live
                            .snapshot
                            .as_deref()
                            .and_then(|s| notice.target(s))
                            .is_some();
                        let drawn = Drawn {
                            key: Key {
                                endpoint: endpoint.id.clone(),
                                generation: endpoint.generation,
                                id: *id,
                            },
                            label: endpoint.label.clone(),
                            inbox: endpoint.connection.inbox.clone(),
                            notice: notice.clone(),
                            corner: if narrow {
                                ToastHerdrPosition::BottomRight
                            } else {
                                notice.position
                            },
                        };
                        (drawn, navigable)
                    })
            })
            .take(visible_limit)
            .collect();
        let now = crate::motion::now();
        self.toast_motion.observe(
            visible.iter().map(|(drawn, _)| drawn.clone()).collect(),
            now,
        );
        if self.toast_motion.moving() {
            window.request_animation_frame();
        }
        let look = |shown, live, navigable| Look {
            shown,
            live,
            navigable,
            max_height: (viewport.height - px(132.)) / visible_limit as f32,
            body_room: (viewport.height - px(208.)).max(px(20.)),
        };
        let mut corners = Vec::new();
        for position in [
            ToastHerdrPosition::TopLeft,
            ToastHerdrPosition::TopRight,
            ToastHerdrPosition::BottomLeft,
            ToastHerdrPosition::BottomRight,
        ] {
            let mut cards: Vec<_> = visible
                .iter()
                .filter(|(drawn, _)| drawn.corner == position)
                .map(|(drawn, navigable)| {
                    let shown = self.toast_motion.shown(&drawn.key, now);
                    let heights = self.toast_motion.heights.clone();
                    let key = drawn.key.clone();
                    // Measured around the card, so the height counts its
                    // border too.
                    div()
                        .relative()
                        .w_full()
                        .flex_none()
                        .child(self.toast_card(drawn, look(shown, true, *navigable), cx))
                        .child(
                            canvas(
                                move |bounds, _, _| {
                                    heights.borrow_mut().insert(key, bounds.size.height);
                                },
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .inset_0(),
                        )
                        .into_any_element()
                })
                .collect();
            // A leaving card keeps its place in the stack, and the room it
            // took closes as it goes, so the cards around it do not jump.
            let mut leaving: Vec<_> = self
                .toast_motion
                .leaving(now)
                .filter(|(gone, _)| gone.drawn.corner == position)
                .collect();
            let bottom = matches!(
                position,
                ToastHerdrPosition::BottomLeft | ToastHerdrPosition::BottomRight
            );
            leaving.sort_by_key(|(gone, _)| if bottom { gone.after } else { gone.before });
            for (gone, left) in leaving {
                let card = self.toast_card(&gone.drawn, look(left, false, false), cx);
                let room = div()
                    .w_full()
                    .flex_none()
                    .overflow_hidden()
                    .when_some(gone.height, |room, height| {
                        room.h(height * left).mb(px(-GAP * (1. - left)))
                    })
                    .child(card);
                let place = match bottom {
                    true => cards.len().saturating_sub(gone.after),
                    false => gone.before.min(cards.len()),
                };
                cards.insert(place, room.into_any_element());
            }
            if !cards.is_empty() {
                corners.push(
                    corner(position, width)
                        .children(cards)
                        .children(self.overlay_probe())
                        .into_any_element(),
                );
            }
        }
        corners
    }
}

/// The space between stacked toasts.
const GAP: f32 = 8.;

/// The column toasts stack in at `position`, `width` wide.
fn corner(position: ToastHerdrPosition, width: Pixels) -> Div {
    use ToastHerdrPosition::*;
    div()
        .absolute()
        .w(width)
        .flex()
        .flex_col()
        .gap(px(GAP))
        .map(|d| match position {
            TopLeft | TopRight => d.top(px(72.)),
            BottomLeft | BottomRight => d.bottom(px(36.)),
        })
        .map(|d| match position {
            TopLeft | BottomLeft => d.left(px(12.)),
            TopRight | BottomRight => d.right(px(12.)),
        })
}

impl HerdrWindow {
    /// Whether toasts, ones still sliding out among them, or the
    /// file-transfer card show now, as their renders decide: nothing shows
    /// over an open menu or in a window too small.
    /// Only Windows hides pages under these.
    #[cfg(windows)]
    pub(crate) fn overlays_shown(&self) -> bool {
        if self.menu.page.is_some() {
            return false;
        }
        self.file_transfer.is_some()
            || (!self.toasts_hidden
                && (self
                    .toast_motion
                    .leaving(crate::motion::now())
                    .next()
                    .is_some()
                    || self.endpoints.iter().any(|endpoint| {
                        endpoint
                            .toasts
                            .entries
                            .iter()
                            .any(|(_, notice)| notice.visible)
                    })))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;
