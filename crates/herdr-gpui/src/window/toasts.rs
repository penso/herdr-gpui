use super::HerdrWindow;
use crate::{
    fonts::StyledFont,
    notifications::{Notice, VISIBLE_LIMIT, safe_text},
};
use gpui::{prelude::*, *};
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

    pub(super) fn render_toasts(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let viewport = window.viewport_size();
        self.toasts_hidden = viewport.height < px(180.) || viewport.width < px(180.);
        self.tick_toasts(
            self.menu.page.is_some() || self.toasts_hidden,
            Instant::now(),
        );
        if self.menu.page.is_some() {
            return Vec::new();
        }
        let viewport = window.viewport_size();
        if viewport.height < px(180.) || viewport.width < px(180.) {
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
                    .map(move |(id, notice)| (endpoint, *id, notice))
            })
            .take(visible_limit)
            .collect();
        [
            ToastHerdrPosition::TopLeft,
            ToastHerdrPosition::TopRight,
            ToastHerdrPosition::BottomLeft,
            ToastHerdrPosition::BottomRight,
        ]
        .into_iter()
        .filter_map(|position| {
            let mut cards = Vec::new();
            for (endpoint, id, notice) in &visible {
                let corner = if narrow {
                    ToastHerdrPosition::BottomRight
                } else {
                    notice.position
                };
                if corner != position {
                    continue;
                }
                let endpoint_id = endpoint.id.clone();
                let inbox = endpoint.connection.inbox.clone();
                let generation = endpoint.generation;
                let id = *id;
                let accent = match notice.kind {
                    SemanticNotificationKind::NeedsAttention => self.theme.palette[3],
                    SemanticNotificationKind::Finished => self.theme.palette[2],
                    _ => self.theme.primary(),
                };
                cards.push(
                    div()
                        .id(SharedString::from(format!(
                            "toast-{}-{generation}-{id}",
                            endpoint.id
                        )))
                        .debug_selector({
                            let endpoint_id = endpoint.id.clone();
                            move || format!("toast-{endpoint_id}-{id}")
                        })
                        .occlude()
                        .w_full()
                        .flex_none()
                        .overflow_hidden()
                        .max_h((viewport.height - px(132.)) / visible_limit as f32)
                        .rounded(px(crate::config::corners::PANEL))
                        .border_1()
                        .border_color(rgb(accent))
                        .bg(crate::config::background(
                            self.theme.surface,
                            self.paint_opacity(),
                        ))
                        .text_color(rgb(self.theme.foreground))
                        .text_font(&self.config.ui)
                        .text_size(px(self.config.ui.size))
                        .p(px(10.))
                        .flex()
                        .gap(px(8.))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                        .on_scroll_wheel(|_, _, cx| cx.stop_propagation())
                        .when(
                            endpoint
                                .live
                                .snapshot
                                .as_deref()
                                .and_then(|s| notice.target(s))
                                .is_some(),
                            |d| d.cursor_pointer(),
                        )
                        .on_click({
                            let endpoint_id = endpoint_id.clone();
                            let inbox = inbox.clone();
                            cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.click_toast(&endpoint_id, generation, &inbox, id, cx);
                            })
                        })
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .flex()
                                .flex_col()
                                .gap(px(4.))
                                .child(
                                    div()
                                        .truncate()
                                        .text_color(rgb(self
                                            .theme
                                            .readable_chrome(self.paint_opacity())
                                            .muted))
                                        .child(safe_text(&endpoint.label, 80)),
                                )
                                .child(div().truncate().child(notice.title.clone()))
                                .when_some(notice.body.clone(), |d, body| {
                                    d.child(div().max_h(px(48.)).overflow_hidden().child(body))
                                }),
                        )
                        .child(
                            div()
                                .id("dismiss")
                                .debug_selector({
                                    let endpoint_id = endpoint.id.clone();
                                    move || format!("toast-dismiss-{endpoint_id}-{id}")
                                })
                                .flex_none()
                                .size(px(24.))
                                .rounded(px(crate::config::corners::CONTROL))
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .hover(|s| s.bg(rgb(self.theme.active)))
                                .child(
                                    svg()
                                        .path("icons/close.svg")
                                        .size(px(12.))
                                        .text_color(rgb(self.theme.foreground)),
                                )
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    cx.stop_propagation();
                                    if let Some(endpoint) = this.endpoints.iter_mut().find(|e| {
                                        e.id == endpoint_id
                                            && e.generation == generation
                                            && std::sync::Arc::ptr_eq(&e.connection.inbox, &inbox)
                                    }) {
                                        endpoint.toasts.dismiss(id);
                                        cx.notify();
                                    }
                                })),
                        ),
                );
            }
            if cards.is_empty() {
                return None;
            }
            Some(
                div()
                    .absolute()
                    .w(width)
                    .flex()
                    .flex_col()
                    .gap(px(8.))
                    .when(
                        matches!(
                            position,
                            ToastHerdrPosition::TopLeft | ToastHerdrPosition::TopRight
                        ),
                        |d| d.top(px(72.)),
                    )
                    .when(
                        matches!(
                            position,
                            ToastHerdrPosition::BottomLeft | ToastHerdrPosition::BottomRight
                        ),
                        |d| d.bottom(px(36.)),
                    )
                    .when(
                        matches!(
                            position,
                            ToastHerdrPosition::TopLeft | ToastHerdrPosition::BottomLeft
                        ),
                        |d| d.left(px(12.)),
                    )
                    .when(
                        matches!(
                            position,
                            ToastHerdrPosition::TopRight | ToastHerdrPosition::BottomRight
                        ),
                        |d| d.right(px(12.)),
                    )
                    .children(cards)
                    .into_any_element(),
            )
        })
        .collect()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::{
        notifications::{Notice, tests::notification},
        sidebar::layout_tests::fixture_window,
    };
    use gpui::{TestAppContext, px, size};
    use std::time::Instant;

    #[gpui::test]
    fn targeted_previews_use_only_current_snapshot_ids(cx: &mut TestAppContext) {
        use herdr_client::protocol::{ClientShellSnapshot, SemanticNotificationKind};
        let (view, cx) = cx.add_window_view(fixture_window);
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

    #[gpui::test]
    fn toast_preview_actions_use_normal_state_without_a_connection(cx: &mut TestAppContext) {
        use crate::actions::ShowToastPreview;
        use herdr_client::protocol::{SemanticNotificationKind, ToastHerdrPosition};

        let (view, cx) = cx.add_window_view(fixture_window);
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
        for (index, kind) in [
            SemanticNotificationKind::NeedsAttention,
            SemanticNotificationKind::Finished,
            SemanticNotificationKind::UpdateInstalled,
            SemanticNotificationKind::Custom,
        ]
        .into_iter()
        .enumerate()
        {
            cx.update(|window, cx| {
                window.dispatch_action(Box::new(ShowToastPreview { kind }), cx);
            });
            cx.update(|window, cx| {
                window.draw(cx).clear(cx);
                let view = view.read(cx);
                let endpoint = &view.endpoints[1];
                let (_, notice) = endpoint.toasts.entries.back().unwrap();
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
            });
            assert!(
                cx.debug_bounds(
                    [
                        "toast-preview-0",
                        "toast-preview-1",
                        "toast-preview-2",
                        "toast-preview-3",
                    ][index]
                )
                .is_some()
            );
        }
        let dismiss = cx.debug_bounds("toast-dismiss-preview-3").unwrap();
        cx.simulate_click(dismiss.center(), Default::default());
        view.update(cx, |view, _| {
            assert!(view.endpoints[1].toasts.entries.is_empty());
        });
    }

    #[gpui::test]
    fn notifications_layout_dismissal_and_focus_are_nonmodal(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                window.focus(&view.focus, cx);
                view.marked = "composition".into();
                view.endpoints[0].toasts.receive((0..3).map(|_| {
                    let mut wire = notification(&"long title ".repeat(100));
                    wire.body = Some("body ".repeat(200));
                    Notice::new(wire, Instant::now()).preview()
                }));
                let mut remote = crate::endpoint::Endpoint::new(
                    "remote".into(),
                    "Remote".into(),
                    herdr_client::ConnectTarget::Socket("/unused-toast-test.sock".into()),
                    true,
                );
                remote
                    .toasts
                    .receive([Notice::new(notification("Remote"), Instant::now()).preview()]);
                view.endpoints.push(remote);
            });
        });
        for (width, height) in [(1000., 600.), (360., 240.)] {
            cx.simulate_resize(size(px(width), px(height)));
            cx.update(|window, cx| {
                window.draw(cx).clear(cx);
                assert!(view.read(cx).focus.is_focused(window));
                assert_eq!(view.read(cx).marked, "composition");
                assert_eq!(view.read(cx).selected_endpoint, 0);
            });
            let mut bottom = px(0.);
            for selector in ["toast-local-0", "toast-local-1", "toast-local-2"]
                .into_iter()
                .take(1)
            {
                let bounds = cx.debug_bounds(selector).unwrap();
                assert!(bounds.left() >= px(0.) && bounds.right() <= px(width));
                assert!(bounds.top() >= bottom && bounds.bottom() <= px(height));
                bottom = bounds.bottom();
                if width == 1000. {
                    assert_eq!(bounds.left(), px(12.));
                } else {
                    assert_eq!(bounds.right(), px(width - 12.));
                }
            }
        }
        // Use a full-height card for the dismissal hit target.
        cx.simulate_resize(size(px(1000.), px(600.)));
        cx.update(|window, cx| window.draw(cx).clear(cx));
        let dismiss = cx.debug_bounds("toast-dismiss-local-0").unwrap();
        cx.simulate_click(dismiss.center(), Default::default());
        cx.update(|window, cx| {
            assert!(view.read(cx).focus.is_focused(window));
            assert_eq!(view.read(cx).endpoints[0].toasts.entries.len(), 2);
            assert_eq!(view.read(cx).endpoints[1].toasts.entries.len(), 1);
            assert_eq!(view.read(cx).marked, "composition");
            view.update(cx, |view, cx| view.open_keybinds(window, cx));
            assert!(
                view.update(cx, |view, cx| view.render_toasts(window, cx))
                    .is_empty()
            );
        });
    }

    #[gpui::test]
    fn notifications_honor_all_corners_and_global_card_limit(cx: &mut TestAppContext) {
        use herdr_client::protocol::ToastHerdrPosition;
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.simulate_resize(size(px(1000.), px(600.)));
        for position in [
            ToastHerdrPosition::TopLeft,
            ToastHerdrPosition::TopRight,
            ToastHerdrPosition::BottomLeft,
            ToastHerdrPosition::BottomRight,
        ] {
            cx.update(|window, cx| {
                view.update(cx, |view, _| {
                    view.endpoints[0].toasts = Default::default();
                    let mut wire = notification("Corner");
                    wire.position = Some(position);
                    view.endpoints[0]
                        .toasts
                        .receive([Notice::new(wire, Instant::now()).preview()]);
                });
                window.draw(cx).clear(cx);
            });
            let bounds = cx.debug_bounds("toast-local-0").unwrap();
            if matches!(
                position,
                ToastHerdrPosition::TopLeft | ToastHerdrPosition::TopRight
            ) {
                assert_eq!(bounds.top(), px(72.));
            } else {
                assert_eq!(bounds.bottom(), px(564.));
            }
            if matches!(
                position,
                ToastHerdrPosition::TopLeft | ToastHerdrPosition::BottomLeft
            ) {
                assert_eq!(bounds.left(), px(12.));
            } else {
                assert_eq!(bounds.right(), px(988.));
            }
        }
        cx.update(|window, cx| {
            view.update(cx, |view, _| {
                for id in ["one", "two", "three"] {
                    let mut endpoint = crate::endpoint::Endpoint::new(
                        id.into(),
                        id.into(),
                        herdr_client::ConnectTarget::Socket("/unused-toast-test.sock".into()),
                        true,
                    );
                    endpoint
                        .toasts
                        .receive([Notice::new(notification(id), Instant::now()).preview()]);
                    view.endpoints.push(endpoint);
                }
            });
            window.draw(cx).clear(cx);
        });
        assert!(cx.debug_bounds("toast-local-0").is_some());
        assert!(cx.debug_bounds("toast-one-0").is_none());
        assert!(cx.debug_bounds("toast-two-0").is_none());
        assert!(cx.debug_bounds("toast-three-0").is_none());
        let dismiss = cx.debug_bounds("toast-dismiss-local-0").unwrap();
        cx.simulate_click(dismiss.center(), Default::default());
        view.read_with(cx, |view, _| {
            assert!(view.endpoints[0].toasts.entries.is_empty());
            assert_eq!(view.endpoints[2].toasts.entries.len(), 1);
            assert_eq!(view.selected_endpoint, 0);
        });
    }
}
