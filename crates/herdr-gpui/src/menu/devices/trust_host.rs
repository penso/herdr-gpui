//! Reviewing the host key of a saved SSH device whose connection SSH refused
//! because it does not trust that key, as on a computer that received the
//! device through machine sync and never connected to it, or after the host
//! was reinstalled. The key is fetched without authenticating and its
//! fingerprints are shown; only an explicit Trust saves it to the user's
//! known hosts file before the device redials. A key that differs from the
//! one trusted before is shown beside that one, with a warning, and Replace
//! swaps them; Enter never replaces a key.

use super::super::Page;
use super::host_key::{Fetched, Key, fetch};
use crate::HerdrWindow;
use crate::i18n::{Localize, fl};
use crate::notifications::safe_text;
use crate::window::Flash;
use gpui::{prelude::*, *};
use herdr_client::{ConnectTarget, HostKeyScan, SshFailure};

/// The open review, for the device selected when it opened. Selecting another
/// device resets the menu, and with it this review and its work.
pub(crate) struct Review {
    /// The endpoint the review was opened for, checked again before it redials.
    endpoint: String,
    pub(super) label: String,
    pub(super) target: String,
    pub(super) step: Step,
    task: Option<Task<()>>,
}

#[cfg(test)]
impl Review {
    /// A review of `target` for `endpoint`, waiting on its key, without
    /// running SSH.
    pub(super) fn fixture(endpoint: &str, target: &str) -> Self {
        Self {
            endpoint: endpoint.into(),
            label: target.into(),
            target: target.into(),
            step: Step::Fetching,
            task: None,
        }
    }
}

#[derive(Debug)]
pub(super) enum Step {
    Fetching,
    Offered(Key),
    Saving(Key),
    /// The key could not be fetched or saved, for this reason.
    Failed(String),
}

impl HerdrWindow {
    /// The SSH target of the selected device, when SSH refused its host key.
    pub(crate) fn untrusted_host(&self) -> Option<&str> {
        let endpoint = &self.endpoints[self.selected_endpoint];
        let ConnectTarget::Ssh { target, .. } = &endpoint.connection.target else {
            return None;
        };
        (endpoint.live.ssh_failure == Some(SshFailure::HostKey)).then_some(target.as_str())
    }

    /// Opens the review for the selected device and fetches the host's key.
    pub(crate) fn review_host_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.untrusted_host().map(str::to_owned) else {
            return;
        };
        if !self.open_menu(window, cx) {
            return;
        }
        let endpoint = &self.endpoints[self.selected_endpoint];
        tracing::info!(category = "host_key", "Reviewing a saved device's host key");
        self.menu.host_key = Some(Review {
            endpoint: endpoint.id.clone(),
            label: endpoint.label.clone(),
            target,
            step: Step::Fetching,
            task: None,
        });
        self.menu.page = Some(Page::HostKey);
        self.fetch_reviewed_host_key(window, cx);
    }

    fn fetch_reviewed_host_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(review) = &mut self.menu.host_key else {
            return;
        };
        review.step = Step::Fetching;
        let target = review.target.clone();
        let background = cx
            .background_executor()
            .spawn(async move { fetch(&target) });
        review.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = background.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.host_key_reviewed(result, window, cx);
            });
        }));
        cx.notify();
    }

    pub(super) fn host_key_reviewed(
        &mut self,
        result: herdr_client::Result<Fetched>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(review) = &mut self.menu.host_key else {
            return;
        };
        if !matches!(review.step, Step::Fetching) {
            return;
        }
        review.task = None;
        review.step = match result {
            Ok(Fetched::Scan(HostKeyScan::New(key))) => Step::Offered(Key::New(key)),
            Ok(Fetched::Changed(key)) => Step::Offered(Key::Changed(key)),
            Ok(Fetched::Scan(HostKeyScan::Refused(failure))) => Step::Failed(failure.localize()),
            // Trusted since the connection failed, as by `ssh` in a terminal:
            // nothing to approve, so the device redials.
            Ok(Fetched::Scan(HostKeyScan::Trusted)) => {
                self.host_key_saved(window, cx);
                return;
            }
            Err(error) => Step::Failed(error.localize()),
        };
        cx.notify();
    }

    /// Saves the offered key, then redials the device. `replace` must be set
    /// for a key that replaces a trusted one, so Enter never does that.
    pub(crate) fn trust_reviewed_host_key(
        &mut self,
        replace: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(review) = &mut self.menu.host_key else {
            return;
        };
        let key = match &review.step {
            Step::Offered(key @ Key::New(_)) => key.clone(),
            Step::Offered(key @ Key::Changed(_)) if replace => key.clone(),
            _ => return,
        };
        tracing::info!(category = "host_key", "Trusting a saved device's host key");
        review.step = Step::Saving(key.clone());
        let background = cx.background_executor().spawn(async move { key.save() });
        review.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = background.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.host_key_trusted(result, window, cx);
            });
        }));
        cx.notify();
    }

    pub(super) fn host_key_trusted(
        &mut self,
        result: herdr_client::Result<()>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(review) = &mut self.menu.host_key else {
            return;
        };
        if !matches!(review.step, Step::Saving(_)) {
            return;
        }
        review.task = None;
        match result {
            Ok(()) => self.host_key_saved(window, cx),
            // The files changed since the key was shown: show what they
            // trust now instead of saving over it.
            Err(herdr_client::Error::Ssh(herdr_client::ssh::Error::Refused(
                SshFailure::HostKey,
            ))) => self.fetch_reviewed_host_key(window, cx),
            Err(error) => {
                review.step = Step::Failed(error.localize());
                cx.notify();
            }
        }
    }

    /// The host is trusted now: close the review and redial the device it
    /// was opened for, if that is still the one selected with this target.
    fn host_key_saved(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(review) = self.menu.host_key.take() else {
            return;
        };
        self.dismiss_menu(window, cx);
        let endpoint = &self.endpoints[self.selected_endpoint];
        let same = endpoint.id == review.endpoint
            && matches!(&endpoint.connection.target,
                ConnectTarget::Ssh { target, .. } if *target == review.target);
        if same {
            self.reconnect(cx);
        }
        self.show_flash(
            Flash::success(fl!(
                "trust-host-trusted",
                host = safe_text(&review.label, 64)
            )),
            cx,
        );
        cx.notify();
    }

    pub(crate) fn render_trust_host(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let review = self.menu.host_key.as_ref()?;
        let theme = &self.theme;
        let host = safe_text(&review.label, 64);
        let target = safe_text(&review.target, 128);
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
        let body = match &review.step {
            Step::Fetching => div()
                .text_color(rgb(theme.muted))
                .child(fl!("trust-host-fetching", target = target.clone()))
                .into_any_element(),
            Step::Offered(key) | Step::Saving(key) => {
                self.render_offered_key(&target, key).into_any_element()
            }
            Step::Failed(error) => div()
                .debug_selector(|| "trust-host-failed".into())
                .text_color(crate::menu::danger(theme))
                .child(fl!(
                    "trust-host-failed",
                    target = target.clone(),
                    error = safe_text(error, 240),
                ))
                .into_any_element(),
        };
        let (offered, saving) = match &review.step {
            Step::Offered(key) => (Some(key), false),
            Step::Saving(key) => (Some(key), true),
            _ => (None, false),
        };
        let mut actions = div().flex().flex_wrap().gap(px(8.)).p(px(8.));
        if let Some(key) = offered {
            let replace = key.replaces();
            let label = match (saving, replace) {
                (true, _) => fl!("trust-host-trusting"),
                (false, false) => fl!("trust-host-trust"),
                (false, true) => fl!("trust-host-replace"),
            };
            actions = actions.child(
                button("trust-host-trust", label, true)
                    .when(replace, |button| {
                        button
                            .bg(crate::menu::danger(theme))
                            .text_color(rgb(theme.surface))
                    })
                    .when(saving, |button| button.opacity(0.6))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.trust_reviewed_host_key(replace, window, cx);
                    })),
            );
        }
        actions = actions.child(
            button(
                "trust-host-dismiss",
                if offered.is_some() {
                    fl!("trust-host-reject")
                } else {
                    fl!("trust-host-close")
                },
                false,
            )
            .on_click(cx.listener(|this, _, window, cx| {
                cx.stop_propagation();
                this.dismiss_menu(window, cx);
            })),
        );
        Some(
            div()
                .debug_selector(|| "trust-host".into())
                .child(
                    div()
                        .p(px(8.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(fl!("trust-host-title", host = host)),
                )
                .child(div().p(px(8.)).child(body))
                .child(actions)
                .into_any_element(),
        )
    }
}
