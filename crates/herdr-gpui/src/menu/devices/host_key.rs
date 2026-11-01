//! Approving an unknown SSH host key from the Add Device dialog. The key is
//! fetched without authenticating, its fingerprint is shown, and only an
//! explicit approval saves it to the user's known hosts file before the host
//! is checked again. A changed key is never offered here: SSH refuses it, and
//! the dialog falls back to the terminal, where the user can investigate.

use super::{
    add_device::{Offer, Setup, Step},
    setup,
};
use crate::HerdrWindow;
use crate::i18n::fl;
use gpui::{prelude::*, *};
use herdr_client::{HostKeyScan, NewHostKey, SshFailure};

impl HerdrWindow {
    /// Fetch the key the host offers, keeping the claim on the host meanwhile.
    pub(super) fn fetch_host_key(
        &mut self,
        request: setup::Request,
        claim: setup::Claim,
        cx: &mut Context<Self>,
    ) {
        let Some(form) = &mut self.menu.device_setup else {
            return;
        };
        tracing::info!(category = "device_setup", "Fetching host key");
        form.step = Step::FetchingKey(request.clone());
        form.claim = Some(claim);
        form.error = None;
        let background = cx
            .background_executor()
            .spawn(async move { herdr_client::scan_host_key(request.target()) });
        form.task = Some(cx.spawn(async move |this, cx| {
            let result = background.await;
            let _ = this.update(cx, |this, cx| this.host_key_fetched(result, cx));
        }));
        cx.notify();
    }

    pub(super) fn host_key_fetched(
        &mut self,
        result: herdr_client::Result<HostKeyScan>,
        cx: &mut Context<Self>,
    ) {
        let Some(Setup {
            step: Step::FetchingKey(request),
            ..
        }) = &self.menu.device_setup
        else {
            return;
        };
        let request = request.clone();
        let target = request.target();
        let error = match result {
            Ok(HostKeyScan::New(key)) => {
                if let Some(form) = &mut self.menu.device_setup {
                    form.step = Step::TrustKey(request, key);
                }
                cx.notify();
                return;
            }
            Ok(HostKeyScan::Refused(SshFailure::HostKey)) => {
                fl!("add-device-key-changed", target = target)
            }
            Ok(HostKeyScan::Refused(failure)) => fl!(
                "add-device-ssh-failed",
                target = target,
                error = failure.to_string(),
            ),
            // The key check passed, so the probe failed for a reason this
            // dialog cannot tell apart; the terminal shows it.
            Ok(HostKeyScan::Trusted) => fl!(
                "add-device-ssh-failed",
                target = target,
                error = SshFailure::HostKey.to_string(),
            ),
            Err(error) => fl!(
                "add-device-key-fetch-failed",
                target = target,
                error = error.to_string(),
            ),
        };
        tracing::warn!(
            category = "device_setup",
            "Host key not offered for approval"
        );
        self.set_device_error(Some(error));
        if let Some(form) = &mut self.menu.device_setup {
            form.step = Step::Confirm(request, Offer::Terminal);
        }
        cx.notify();
    }

    /// Save the approved key, then check the host again with it trusted.
    pub(super) fn trust_host_key(&mut self, cx: &mut Context<Self>) {
        let Some(form) = &mut self.menu.device_setup else {
            return;
        };
        let Step::TrustKey(request, key) = &form.step else {
            return;
        };
        let (request, key) = (request.clone(), key.clone());
        let claim = form.claim.take();
        tracing::info!(category = "device_setup", "Trusting host key");
        form.step = Step::Checking(request.clone());
        form.error = None;
        let background = cx.background_executor().spawn(async move {
            let claim = match claim {
                Some(claim) => claim,
                None => setup::claim(&request)?,
            };
            let probe = herdr_client::trust_host_key(&key)
                .map_err(crate::Error::from)
                .and_then(|()| request.probe());
            Ok::<_, crate::Error>((claim, probe))
        });
        form.task = Some(cx.spawn(async move |this, cx| {
            let result = background.await;
            let _ = this.update(cx, |this, cx| this.device_probed(result, cx));
        }));
        cx.notify();
    }

    /// The fingerprints to compare, and where an approved key is saved.
    pub(super) fn render_host_key(&self, target: &str, key: &NewHostKey) -> impl IntoElement {
        let theme = &self.theme;
        let mut fingerprints = div()
            .debug_selector(|| "device-setup-fingerprints".into())
            .flex()
            .flex_col()
            .gap(px(4.))
            .p(px(8.))
            .rounded(px(crate::config::corners::CONTROL))
            .bg(rgb(theme.active))
            .font_family(self.config.terminal.family.clone());
        for key in &key.fingerprints {
            fingerprints = fingerprints.child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_x(px(8.))
                    .child(
                        div()
                            .text_color(rgb(theme.muted))
                            .child(format!("{} {}", key.kind, key.bits)),
                    )
                    .child(div().min_w_0().child(key.fingerprint.clone())),
            );
        }
        // The host's own copy of the first key, named the way OpenSSH does.
        let host_file = key.fingerprints.first().map_or_else(
            || "/etc/ssh/ssh_host_ed25519_key.pub".to_owned(),
            |key| {
                format!(
                    "/etc/ssh/ssh_host_{}_key.pub",
                    key.kind.to_ascii_lowercase()
                )
            },
        );
        div()
            .debug_selector(|| "device-setup-host-key".into())
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(fl!("add-device-key-unknown", target = target))
            .child(fingerprints)
            .child(div().text_color(rgb(theme.muted)).child(fl!(
                "add-device-key-check",
                host_file = host_file,
                known_hosts = key.known_hosts().display().to_string(),
            )))
    }
}
