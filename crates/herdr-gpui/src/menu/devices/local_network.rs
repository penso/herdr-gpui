//! Local Network permission in the Add Device dialog. macOS refuses the
//! first local network operation while its alert waits for an answer, and
//! there is no API to ask or read the permission (Apple's TN3179). So when a
//! host check is refused, the dialog keeps the host claimed and waits for the
//! user, then checks the host again; nothing has to be restarted.

use super::{add_device::Step, setup};
use crate::HerdrWindow;
use crate::i18n::fl;
use gpui::{prelude::*, *};
use herdr_client::{HostProbe, LocalNetworkAccess, SshFailure};
use std::{sync::Arc, time::Duration};

/// System Settings › Privacy & Security › Local Network.
const SETTINGS_URL: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_LocalNetwork"; // i18n: data
/// How long a refused host check waits for the user to allow Herdr.
const PERMISSION_WAIT: Duration = Duration::from_secs(60);

impl HerdrWindow {
    /// A button that opens the Local Network settings. Only macOS has them.
    pub(super) fn local_network_settings_button(
        &self,
        id: &'static str,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        if !cfg!(target_os = "macos") {
            return None;
        }
        let theme = &self.theme;
        Some(
            div()
                .id(id)
                .debug_selector(move || id.into())
                .flex_none()
                .px_2()
                .py_1()
                .cursor_pointer()
                .rounded(px(crate::config::corners::CONTROL))
                .hover(|s| s.bg(rgb(theme.active)))
                .child(fl!("add-device-local-network-settings"))
                .on_click(cx.listener(|_, _, _, cx| cx.open_url(SETTINGS_URL))),
        )
    }

    /// The host check was refused the local network. Wait for the user to
    /// allow Herdr, then check the host again with the same claim. Asking
    /// from this long-lived app is also what keeps the system alert up: the
    /// `ssh` that was refused exits too soon to show it. Cancelling drops the
    /// task, which ends the wait and releases the claim.
    pub(super) fn await_local_network(
        &mut self,
        request: setup::Request,
        claim: setup::Claim,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = &mut self.menu.device_setup else {
            return;
        };
        tracing::info!(
            category = "device_setup",
            "Waiting for Local Network permission"
        );
        form.step = Step::AwaitingLocalNetwork(request.clone());
        form.error = None;
        let alive = Arc::new(());
        let watched = Arc::downgrade(&alive);
        let background = cx.background_executor().spawn(async move {
            let access = herdr_client::wait_for_local_network(PERMISSION_WAIT, || {
                watched.strong_count() > 0
            });
            let probe = match access {
                LocalNetworkAccess::Denied => {
                    Ok(HostProbe::SshFailed(SshFailure::LocalNetworkDenied))
                }
                LocalNetworkAccess::Allowed | LocalNetworkAccess::Unknown => request.probe(),
            };
            Ok::<_, crate::Error>((claim, probe))
        });
        form.task = Some(cx.spawn(async move |this, cx| {
            let result = background.await;
            drop(alive);
            let _ = this.update(cx, |this, cx| this.device_probed(result, cx));
        }));
        cx.notify();
    }
}

/// What the dialog says while it waits.
pub(super) fn waiting_status(target: &str) -> String {
    fl!("add-device-local-network-waiting", target = target)
}
