use super::HerdrWindow;
use crate::notifications::{Notice, VISIBLE_LIMIT, safe_text};
use gpui_kit::component::{
    ActiveTheme, WindowExt,
    notification::{Notification, NotificationType},
};
use gpui_kit::{
    Anchor, Context, ElementId, InteractiveElement, IntoElement, ParentElement, SharedString,
    Styled, Window, div, prelude::FluentBuilder, px,
};
use herdr_client::protocol::{SemanticNotification, SemanticNotificationKind, ToastHerdrPosition};
use std::{sync::TryLockError, task::Poll, time::Instant};

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
        let target = match self.toast_target(index, id) {
            Poll::Ready(target) => target,
            Poll::Pending => {
                // The displayed target can start a handoff, but only a fresh
                // inbox validation may queue navigation after contention ends.
                let endpoint = &self.endpoints[index];
                endpoint.live.snapshot.as_deref().and_then(|snapshot| {
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
        if !self.select_endpoint(endpoint_id, cx) {
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
        if !notice.visible || (!accepted && notice.expires <= Instant::now()) {
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
                .snapshot
                .as_deref()
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
        .with_snapshot(snapshot.as_deref())
        .preview();
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
            self.pending_toast,
            now,
        )
    }

    /// Presents the scheduler's visible notices as kit notifications and
    /// retires the ones it no longer shows. The notice model stays the single
    /// authority: its expiry, pausing, and queueing decide what is up, and a
    /// kit card only ever mirrors one visible notice.
    pub(super) fn sync_toasts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let viewport = window.viewport_size();
        self.toasts_hidden = viewport.height < px(180.) || viewport.width < px(180.);
        let hidden = self.menu.page.is_some() || self.toasts_hidden;
        self.tick_toasts(hidden, Instant::now());
        let narrow = viewport.width < px(720.);
        let wanted: Vec<ToastCard> = if hidden {
            Vec::new()
        } else {
            self.endpoints
                .iter()
                .flat_map(|endpoint| {
                    endpoint
                        .toasts
                        .entries
                        .iter()
                        .filter(|(_, notice)| notice.visible)
                        .map(|(id, _)| ToastCard {
                            endpoint: endpoint.id.clone(),
                            generation: endpoint.generation,
                            id: *id,
                        })
                })
                .take(VISIBLE_LIMIT)
                .collect()
        };
        // Dropped before the kit closes it, so its close callback knows the
        // model retired it and leaves the notice alone.
        let mut cards = std::mem::take(&mut self.toast_cards);
        cards.retain(|card| {
            let keep = wanted.contains(card);
            if !keep {
                window.remove_notification1::<DaemonToast>(card.key(), cx);
            }
            keep
        });
        for card in wanted {
            if cards.contains(&card) {
                continue;
            }
            if let Some(notification) = self.toast_notification(&card, narrow, cx) {
                window.push_notification(notification, cx);
            }
            cards.push(card);
        }
        self.toast_cards = cards;
    }

    fn toast_notification(
        &self,
        card: &ToastCard,
        narrow: bool,
        cx: &mut Context<Self>,
    ) -> Option<Notification> {
        let endpoint = self
            .endpoints
            .iter()
            .find(|e| e.id == card.endpoint && e.generation == card.generation)?;
        let (_, notice) = endpoint
            .toasts
            .entries
            .iter()
            .find(|(id, _)| *id == card.id)?;
        let kind = match notice.kind {
            SemanticNotificationKind::NeedsAttention => NotificationType::Warning,
            SemanticNotificationKind::Finished => NotificationType::Success,
            SemanticNotificationKind::UpdateInstalled | SemanticNotificationKind::Custom => {
                NotificationType::Info
            }
        };
        // Narrow windows have no room for a card beside the terminal's edge.
        let placement = match if narrow {
            ToastHerdrPosition::BottomRight
        } else {
            notice.position
        } {
            ToastHerdrPosition::TopLeft => Anchor::TopLeft,
            ToastHerdrPosition::TopRight => Anchor::TopRight,
            ToastHerdrPosition::BottomLeft => Anchor::BottomLeft,
            ToastHerdrPosition::BottomRight => Anchor::BottomRight,
        };
        let origin: SharedString = safe_text(&endpoint.label, 80).into();
        let selector = format!("toast-{}-{}", card.endpoint, card.id);
        let inbox = endpoint.connection.inbox.clone();
        let view = cx.entity().downgrade();
        let clicked = card.clone();
        let closed = card.clone();
        Some(
            Notification::new()
                .id1::<DaemonToast>(card.key())
                .with_type(kind)
                .title(notice.title.clone())
                .when_some(notice.body.clone(), |note, body| note.message(body))
                .placement(placement)
                // The notice model owns expiry, including pausing it while
                // hidden or while its navigation is pending.
                .autohide(false)
                .content(move |_, _, cx| {
                    let selector = selector.clone();
                    div()
                        .debug_selector(move || selector)
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .truncate()
                        .child(origin.clone())
                        .into_any_element()
                })
                .on_click({
                    let view = view.clone();
                    move |_, _, cx| {
                        let _ = view.update(cx, |this, cx| {
                            this.click_toast(
                                &clicked.endpoint,
                                clicked.generation,
                                &inbox,
                                clicked.id,
                                cx,
                            );
                        });
                    }
                })
                .on_close(move |_, cx| {
                    let _ = view.update(cx, |this, cx| this.toast_closed(&closed, cx));
                }),
        )
    }

    /// The user closed a card. A notice whose navigation is still pending
    /// stays until the handoff settles it; any other leaves the model, as the
    /// old dismiss button did.
    pub(crate) fn toast_closed(&mut self, card: &ToastCard, cx: &mut Context<Self>) {
        if !self.toast_cards.contains(card) || self.pending_toast == Some(card.id) {
            return;
        }
        if let Some(endpoint) = self
            .endpoints
            .iter_mut()
            .find(|e| e.id == card.endpoint && e.generation == card.generation)
        {
            endpoint.toasts.dismiss(card.id);
            cx.notify();
        }
    }
}

/// Type key of the kit notifications that mirror daemon notices.
struct DaemonToast;

/// One visible notice presented as a kit notification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ToastCard {
    endpoint: String,
    generation: u64,
    id: u64,
}

impl ToastCard {
    fn key(&self) -> ElementId {
        SharedString::from(format!("{}-{}-{}", self.endpoint, self.generation, self.id)).into()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::{
        notifications::{Notice, tests::notification},
        sidebar::layout_tests::fixture_window,
    };
    use gpui_kit::{TestAppContext, px, size};
    use std::time::Instant;

    #[gpui_kit::test]
    fn targeted_previews_use_only_current_snapshot_ids(cx: &mut TestAppContext) {
        use herdr_client::protocol::{ClientShellSnapshot, SemanticNotificationKind};
        let (view, cx) = crate::test_support::add_window_view(cx, fixture_window);
        let snapshot: ClientShellSnapshot = serde_json::from_str(include_str!(
            "../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
        ))
        .unwrap();
        view.update(cx, |view, cx| {
            view.endpoints[0].live.snapshot = Some(std::sync::Arc::new(snapshot.clone()));
            view.config.notifications.delay_seconds = 3600;
            for kind in [
                SemanticNotificationKind::NeedsAttention,
                SemanticNotificationKind::Finished,
                SemanticNotificationKind::Custom,
            ] {
                view.show_toast_preview(kind, cx);
                let notice = &view.endpoints[0].toasts.entries.back().unwrap().1;
                if kind == SemanticNotificationKind::Custom {
                    assert!(notice.target(&snapshot).is_none());
                } else {
                    assert_eq!(
                        notice.target(&snapshot),
                        Some(crate::navigation::NavigationTarget::Pane("w1:p1"))
                    );
                }
            }
        });
    }

    #[gpui_kit::test]
    fn toast_preview_actions_use_normal_state_without_a_connection(cx: &mut TestAppContext) {
        use crate::actions::ShowToastPreview;
        use herdr_client::protocol::{SemanticNotificationKind, ToastHerdrPosition};

        let (view, cx) = crate::test_support::add_window_view(cx, fixture_window);
        cx.simulate_resize(size(px(1000.), px(600.)));
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                window.focus(&view.focus, cx);
                view.endpoints.push(crate::endpoint::Endpoint::new(
                    "preview".into(),
                    "Preview host".into(),
                    herdr_client::ConnectTarget::Socket("/unused-toast-preview.sock".into()),
                    false,
                ));
                view.selected_endpoint = 1;
            });
            window.draw(cx).clear(cx);
        });
        let snapshot = view.read_with(cx, |view, _| view.live.snapshot.clone());
        let updater = view.read_with(cx, |view, _| view.updater.state().clone());
        for kind in [
            SemanticNotificationKind::NeedsAttention,
            SemanticNotificationKind::Finished,
            SemanticNotificationKind::UpdateInstalled,
            SemanticNotificationKind::Custom,
        ] {
            cx.update(|window, cx| {
                window.dispatch_action(Box::new(ShowToastPreview { kind }), cx);
            });
            cx.update(|window, cx| {
                window.draw(cx).clear(cx);
                let view = view.read(cx);
                let endpoint = &view.endpoints[1];
                let (id, notice) = endpoint.toasts.entries.back().unwrap();
                assert_eq!(notice.kind, kind);
                assert!(!notice.title.is_empty());
                assert!(notice.body.as_ref().unwrap().starts_with("QA preview"));
                assert_eq!(notice.position, ToastHerdrPosition::BottomRight);
                assert_eq!(endpoint.toasts.entries.len(), 1);
                assert!(endpoint.connection.handle.is_none());
                assert!(view.endpoints[0].toasts.entries.is_empty());
                assert_eq!(view.selected_endpoint, 1);
                assert_eq!(view.live.snapshot, snapshot);
                assert_eq!(view.updater.state(), &updater);
                assert!(view.focus.is_focused(window));
                // Each preview replaces the last card rather than stacking.
                assert_eq!(view.toast_cards.len(), 1);
                assert_eq!(view.toast_cards[0].id, *id);
            });
        }
    }

    #[gpui_kit::test]
    fn visible_notices_become_one_kit_card_that_a_close_dismisses(cx: &mut TestAppContext) {
        use gpui_kit::component::WindowExt;
        let (view, cx) = crate::test_support::add_window_view(cx, fixture_window);
        cx.simulate_resize(size(px(1000.), px(600.)));
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                window.focus(&view.focus, cx);
                view.marked = "composition".into();
                view.endpoints[0].toasts.receive(
                    ["first", "second"]
                        .map(|title| Notice::new(notification(title), Instant::now()).preview()),
                );
            });
            window.draw(cx).clear(cx);
            assert_eq!(window.notifications(cx).len(), 1);
            // Presenting a notice leaves focus and composition alone.
            assert!(view.read(cx).focus.is_focused(window));
            assert_eq!(view.read(cx).marked, "composition");
        });
        let card = view.read_with(cx, |view, _| view.toast_cards[0].clone());
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
            // Redrawing never pushes the same notice twice.
            assert_eq!(window.notifications(cx).len(), 1);
            view.update(cx, |view, cx| view.toast_closed(&card, cx));
            let entries = &view.read(cx).endpoints[0].toasts.entries;
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].1.title, "second");
            window.draw(cx).clear(cx);
            let cards = &view.read(cx).toast_cards;
            assert_eq!(cards.len(), 1);
            assert_ne!(cards[0], card);
        });
        // A card the model retired closes without touching the model again.
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.endpoints[0].toasts = Default::default();
                cx.notify();
            });
            window.draw(cx).clear(cx);
            assert!(view.read(cx).toast_cards.is_empty());
        });
    }
}
