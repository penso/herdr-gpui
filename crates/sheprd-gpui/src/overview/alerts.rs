//! The Recent alerts shelf: what each host's notification history kept, newest
//! first, where clicking one goes to its pane.

use super::{presets, view::Frame};
use crate::{
    SheprdWindow,
    i18n::fl,
    navigation::OwnedNavigationTarget,
    notifications::{Alert, HISTORY_LIMIT},
    sidebar::ActivityPanel,
    window::Flash,
};
use gpui::{prelude::*, *};
use sheprd_client::protocol::{AgentStatus, SemanticNotificationKind as Kind};
use sheprd_control_protocol::protocol::app::AlertInfo;
use std::time::UNIX_EPOCH;

const ROW_HEIGHT: f32 = 36.;

/// One alert and the host that sent it.
pub(crate) struct Listed<'a> {
    pub endpoint: &'a str,
    pub host: &'a str,
    pub alert: &'a Alert,
}

impl SheprdWindow {
    /// Every host's alerts, newest first.
    pub(crate) fn overview_alerts(&self) -> Vec<Listed<'_>> {
        let mut alerts: Vec<_> = self
            .endpoints
            .iter()
            .flat_map(|endpoint| {
                endpoint.toasts.history.alerts().map(|alert| Listed {
                    endpoint: &endpoint.id,
                    host: &endpoint.label,
                    alert,
                })
            })
            .collect();
        alerts.sort_by_key(|listed| std::cmp::Reverse(listed.alert.at));
        alerts.truncate(HISTORY_LIMIT);
        alerts
    }

    /// The alerts as `sheprd window alerts` reports them.
    pub(crate) fn alert_infos(&self) -> Vec<AlertInfo> {
        self.overview_alerts()
            .into_iter()
            .map(|listed| {
                let notice = &listed.alert.notice;
                AlertInfo {
                    host: listed.host.to_owned(),
                    needs_attention: notice.kind == Kind::NeedsAttention,
                    title: notice.title.clone(),
                    body: notice.body.clone(),
                    pane: notice.pane_id.clone(),
                    at: listed
                        .alert
                        .at
                        .duration_since(UNIX_EPOCH)
                        .map_or(0, |elapsed| elapsed.as_secs()),
                }
            })
            .collect()
    }

    /// Moves when any host keeps another alert.
    pub(super) fn overview_alerts_revision(&self) -> u64 {
        self.endpoints
            .iter()
            .map(|endpoint| endpoint.toasts.history.revision())
            .sum()
    }

    /// Opens the Overview on its alerts, for the palette's command.
    pub(crate) fn show_notification_history(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_activity_panel(ActivityPanel::Overview, window, cx);
        if self.overview_alerts().is_empty() {
            self.show_flash(Flash::warning(fl!("overview-alerts-empty")), cx);
            return;
        }
        self.overview.alerts_open = true;
        // The shelf is the last thing on the page.
        self.overview.scroll.scroll_to_bottom();
        cx.notify();
    }

    pub(super) fn overview_alert(
        &self,
        frame: &Frame,
        listed: Listed,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let look = &frame.look;
        let theme = look.theme;
        let Listed {
            endpoint,
            host,
            alert,
        } = listed;
        let id = alert.id;
        let selector = format!("overview-alert-{endpoint}-{id}");
        let status = match alert.notice.kind {
            Kind::NeedsAttention => AgentStatus::Blocked,
            _ => AgentStatus::Done,
        };
        // A described body's first line is the task; the rest repeat the place.
        let detail = alert
            .notice
            .body
            .as_deref()
            .and_then(|body| body.lines().next())
            .unwrap_or_default()
            .to_owned();
        let clicked = endpoint.to_owned();
        div()
            .id(SharedString::from(selector.clone()))
            .debug_selector(move || selector.clone())
            .h(px(ROW_HEIGHT))
            .flex_none()
            .px_2()
            .flex()
            .items_center()
            .gap_3()
            .rounded(px(crate::config::corners::CONTROL))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(theme.active)))
            .child(
                div()
                    .w(px(12.))
                    .flex()
                    .justify_center()
                    .child(look.dot(status)),
            )
            .child(
                div()
                    .flex_none()
                    .max_w(px(320.))
                    .truncate()
                    .font_weight(FontWeight::MEDIUM)
                    .child(alert.notice.title.clone()),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(look.small())
                    .text_color(rgb(theme.muted))
                    .child(detail),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(look.small())
                    .text_color(rgb(theme.muted))
                    .child(host.to_owned()),
            )
            .child(look.mono(presets::elapsed(alert.at, frame.now)))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_overview_alert(&clicked, id, window, cx);
            }))
            .into_any_element()
    }

    /// Where an alert points now, if its daemon boot and pane still stand.
    fn overview_alert_target(
        &self,
        endpoint: &str,
        id: u64,
    ) -> Option<(OwnedNavigationTarget, Option<String>, Option<String>)> {
        let (index, host) = self
            .endpoints
            .iter()
            .enumerate()
            .find(|(_, host)| host.id == endpoint)?;
        let live = if index == self.selected_endpoint {
            &self.live
        } else {
            &host.live
        };
        let snapshot = live
            .snapshot
            .as_deref()
            .filter(|_| host.enabled && live.status.is_connected())?;
        let notice = &host.toasts.history.get(id)?.notice;
        let target = notice.target(snapshot)?;
        Some((
            (&target).into(),
            notice.tab_id.clone(),
            notice.workspace_id.clone(),
        ))
    }

    fn open_overview_alert(
        &mut self,
        endpoint: &str,
        id: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((target, tab, workspace)) = self.overview_alert_target(endpoint, id) else {
            self.show_flash(Flash::warning(fl!("overview-alert-gone")), cx);
            return;
        };
        // Read before navigating, which may switch devices.
        let on_screen = self.endpoints[self.selected_endpoint].id == endpoint
            && self
                .browser_key()
                .is_some_and(|(_, shown)| Some(shown) == workspace);
        self.show_activity_panel(ActivityPanel::Workspaces, window, cx);
        self.activity.return_to = Some(ActivityPanel::Overview);
        self.navigate_endpoint(endpoint, target.as_deref(), cx);
        // An alert in the tab already focused moves nothing, so show it here.
        if let (true, Some(tab)) = (on_screen, tab) {
            self.terminal_focus_moved(&tab, false, cx);
        }
        window.focus(&self.focus, cx);
        cx.notify();
    }
}
