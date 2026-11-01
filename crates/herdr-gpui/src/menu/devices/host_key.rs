//! Approving an SSH host key from the Add Device dialog, and the pieces the
//! saved device review (`trust_host`) shares. The key is fetched without
//! authenticating, its fingerprints are shown, and only an explicit approval
//! saves it to the user's known hosts file before the host is checked again.
//! A key that differs from the one trusted before is shown beside that one
//! under a warning, and approving it replaces the old entries. A key trusted
//! system-wide or pinned by a marker line falls back to the terminal.

use super::{
    add_device::{Offer, Setup, Step},
    setup,
};
use crate::HerdrWindow;
use crate::i18n::{Localize, fl};
use gpui::{prelude::*, *};
use herdr_client::{ChangedHostKey, HostKeyFingerprint, HostKeyScan, NewHostKey, SshFailure};

/// A key on offer: one no file lists yet, or one replacing a trusted key.
#[derive(Clone, Debug)]
pub(super) enum Key {
    New(NewHostKey),
    Changed(ChangedHostKey),
}

impl Key {
    /// Saves the key: appended when new, or in place of the trusted one.
    /// Blocks on disk and `ssh-keygen`: call it from a background thread.
    pub(super) fn save(&self) -> herdr_client::Result<()> {
        match self {
            Self::New(key) => herdr_client::trust_host_key(key),
            Self::Changed(key) => herdr_client::replace_host_key(key),
        }
    }

    /// Whether saving replaces a key the user trusted before.
    pub(super) fn replaces(&self) -> bool {
        matches!(self, Self::Changed(_))
    }
}

/// What fetching found: the first check, or the changed key it led to.
#[derive(Debug)]
pub(super) enum Fetched {
    Scan(HostKeyScan),
    Changed(ChangedHostKey),
}

/// Fetches the key `target` offers; a key that differs from a trusted one is
/// fetched again with the trusted keys beside it. Blocks on SSH.
pub(super) fn fetch(target: &str) -> herdr_client::Result<Fetched> {
    match herdr_client::scan_host_key(target)? {
        HostKeyScan::Refused(SshFailure::HostKey) => {
            herdr_client::scan_changed_host_key(target).map(Fetched::Changed)
        }
        scan => Ok(Fetched::Scan(scan)),
    }
}

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
            .spawn(async move { fetch(request.target()) });
        form.task = Some(cx.spawn(async move |this, cx| {
            let result = background.await;
            let _ = this.update(cx, |this, cx| this.host_key_fetched(result, cx));
        }));
        cx.notify();
    }

    pub(super) fn host_key_fetched(
        &mut self,
        result: herdr_client::Result<Fetched>,
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
            Ok(Fetched::Scan(HostKeyScan::New(key))) => {
                self.offer_host_key(request, Key::New(key), cx);
                return;
            }
            Ok(Fetched::Changed(key)) => {
                self.offer_host_key(request, Key::Changed(key), cx);
                return;
            }
            Ok(Fetched::Scan(HostKeyScan::Refused(failure))) => fl!(
                "add-device-ssh-failed",
                target = target,
                error = failure.localize(),
            ),
            // The key check passed, so the probe failed for a reason this
            // dialog cannot tell apart; the terminal shows it.
            Ok(Fetched::Scan(HostKeyScan::Trusted)) => fl!(
                "add-device-ssh-failed",
                target = target,
                error = SshFailure::HostKey.localize(),
            ),
            Err(error) => fl!(
                "add-device-key-fetch-failed",
                target = target,
                error = error.localize(),
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

    fn offer_host_key(&mut self, request: setup::Request, key: Key, cx: &mut Context<Self>) {
        if let Some(form) = &mut self.menu.device_setup {
            form.step = Step::TrustKey(request, key);
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
            let probe = key
                .save()
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

    /// The key on offer: a new one's fingerprints, or a changed one beside
    /// the key trusted before.
    pub(super) fn render_offered_key(&self, target: &str, key: &Key) -> AnyElement {
        match key {
            Key::New(key) => self.render_host_key(target, key).into_any_element(),
            Key::Changed(key) => self.render_changed_key(target, key).into_any_element(),
        }
    }

    /// The warning, the key trusted before, and the key offered now.
    fn render_changed_key(&self, target: &str, key: &ChangedHostKey) -> impl IntoElement {
        let theme = &self.theme;
        div()
            .debug_selector(|| "trust-host-changed".into())
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(
                div()
                    .text_color(crate::menu::danger(theme))
                    .child(fl!("trust-host-changed", target = target)),
            )
            .child(
                div()
                    .text_color(rgb(theme.muted))
                    .child(fl!("trust-host-before")),
            )
            .child(self.fingerprint_list("trust-host-trusted-keys", &key.trusted))
            .child(
                div()
                    .text_color(rgb(theme.muted))
                    .child(fl!("trust-host-now")),
            )
            .child(self.fingerprint_list("trust-host-offered-keys", &key.fingerprints))
            .child(div().text_color(rgb(theme.muted)).child(fl!(
                "trust-host-replace-note",
                known_hosts = key.known_hosts().display().to_string(),
            )))
    }

    fn fingerprint_list(&self, id: &'static str, keys: &[HostKeyFingerprint]) -> Div {
        let theme = &self.theme;
        div()
            .debug_selector(move || id.into())
            .flex()
            .flex_col()
            .gap(px(4.))
            .p(px(8.))
            .rounded(px(crate::config::corners::CONTROL))
            .bg(rgb(theme.active))
            .font_family(self.config.terminal.family.clone())
            .children(keys.iter().map(|key| {
                div()
                    .flex()
                    .flex_wrap()
                    .gap_x(px(8.))
                    .child(
                        div()
                            .text_color(rgb(theme.muted))
                            .child(format!("{} {}", key.kind, key.bits)),
                    )
                    .child(div().min_w_0().child(key.fingerprint.clone()))
            }))
    }
}
