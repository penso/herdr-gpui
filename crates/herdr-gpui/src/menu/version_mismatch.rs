//! The notice that a daemon refused this app's handshake and which side must
//! be updated. The update command is shown and copied, never run: updating a
//! daemon may stop its sessions, so the user decides when.

// i18n: migrated
use super::Page;
use crate::i18n::fl;
use crate::{HerdrWindow, fonts::StyledFont, notifications::safe_text, window::Flash};
use gpui::{prelude::*, *};
use herdr_client::{ConnectTarget, MIN_HERDR_VERSION, VersionMismatch};

pub(crate) const UPDATE_COMMAND: &str = "herdr update"; // i18n: data

/// What the open notice describes, captured as it opens so a retry that
/// resets the connection cannot change the text under the reader.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Notice {
    /// The SSH host's label, or `None` for this device.
    pub(crate) host: Option<String>,
    pub(crate) mismatch: VersionMismatch,
}

impl Notice {
    pub(crate) fn title(&self) -> String {
        match self.mismatch {
            VersionMismatch::DaemonOutdated { .. } => fl!("version-mismatch-daemon-title"),
            VersionMismatch::ClientOutdated { .. } => fl!("version-mismatch-client-title"),
        }
    }

    pub(crate) fn body(&self) -> String {
        let (place, host) = match &self.host {
            Some(host) => ("host", safe_text(host, 64)),
            None => ("local", String::new()),
        };
        // Releases before the endpoint protocol report no version to this
        // app, so only the requirement can be stated, not their age.
        match (&self.mismatch, self.mismatch.server_version()) {
            (VersionMismatch::DaemonOutdated { .. }, Some(version)) => fl!(
                "version-mismatch-daemon-old",
                place = place,
                host = host,
                version = version,
            ),
            (VersionMismatch::DaemonOutdated { .. }, None) => fl!(
                "version-mismatch-daemon-silent",
                place = place,
                host = host,
                minimum = MIN_HERDR_VERSION,
            ),
            (VersionMismatch::ClientOutdated { .. }, Some(version)) => fl!(
                "version-mismatch-client-old",
                place = place,
                host = host,
                version = version,
            ),
            (VersionMismatch::ClientOutdated { .. }, None) => fl!(
                "version-mismatch-client-old-unknown",
                place = place,
                host = host,
            ),
        }
    }
}

impl HerdrWindow {
    /// Announces the selected endpoint's refused handshake once, when no other
    /// popup is open; retries that are refused again stay quiet.
    pub(crate) fn announce_version_mismatch(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.endpoints[self.selected_endpoint]
            .version_mismatch
            .is_none()
        {
            self.version_notice_shown = false;
        } else if !self.version_notice_shown && self.menu.page.is_none() {
            self.version_notice_shown = true;
            self.show_version_mismatch(window, cx);
        }
    }

    /// Opens the notice for the selected endpoint's refused handshake.
    pub(crate) fn show_version_mismatch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let endpoint = &self.endpoints[self.selected_endpoint];
        let Some(mismatch) = endpoint.version_mismatch.clone() else {
            return;
        };
        let host = matches!(
            endpoint.connection.target,
            ConnectTarget::Ssh { .. } | ConnectTarget::Wsl { .. }
        )
        .then(|| endpoint.label.clone());
        if !self.open_menu(window, cx) {
            return;
        }
        self.menu.version_notice = Some(Notice { host, mismatch });
        self.menu.page = Some(Page::VersionMismatch);
    }

    /// The notice's primary action, also bound to Enter.
    pub(super) fn act_on_version_mismatch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(notice) = &self.menu.version_notice else {
            return;
        };
        match notice.mismatch {
            VersionMismatch::DaemonOutdated { .. } => {
                cx.write_to_clipboard(ClipboardItem::new_string(UPDATE_COMMAND.into()));
                self.show_flash(
                    Flash::success(fl!("version-mismatch-copied", command = UPDATE_COMMAND)),
                    cx,
                );
                self.dismiss_menu(window, cx);
            }
            VersionMismatch::ClientOutdated { .. } => self.open_app_update(false, window, cx),
        }
    }

    pub(super) fn render_version_mismatch(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let Some(notice) = &self.menu.version_notice else {
            return div();
        };
        let button = |id: &'static str, label: String, primary: bool| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .p(px(8.))
                .rounded(px(crate::config::corners::CONTROL))
                .when(primary, |button| button.bg(rgb(theme.active)))
                .when(!primary, |button| button.hover(|s| s.bg(rgb(theme.active))))
                .cursor_pointer()
                .child(label)
        };
        let daemon = matches!(notice.mismatch, VersionMismatch::DaemonOutdated { .. });
        div()
            .debug_selector(|| "version-mismatch".into())
            .child(
                div()
                    .debug_selector(|| "version-mismatch-title".into())
                    .p(px(8.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(notice.title()),
            )
            .child(
                div()
                    .debug_selector(|| "version-mismatch-body".into())
                    .p(px(8.))
                    .child(notice.body()),
            )
            .when(daemon, |notice| {
                notice.child(
                    div()
                        .debug_selector(|| "version-mismatch-command".into())
                        .mx(px(8.))
                        .p(px(8.))
                        .rounded(px(crate::config::corners::CONTROL))
                        .bg(rgb(theme.active))
                        .text_font(&self.config.terminal)
                        .child(UPDATE_COMMAND),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .p(px(8.))
                    .child(
                        button(
                            "version-mismatch-action",
                            if daemon {
                                fl!("version-mismatch-copy")
                            } else {
                                fl!("version-mismatch-check")
                            },
                            true,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.act_on_version_mismatch(window, cx);
                        })),
                    )
                    .child(
                        button(
                            "version-mismatch-dismiss",
                            fl!("version-mismatch-dismiss"),
                            false,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.dismiss_menu(window, cx);
                        })),
                    ),
            )
    }
}
