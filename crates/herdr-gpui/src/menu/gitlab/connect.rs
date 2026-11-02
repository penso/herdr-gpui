//! Connecting a GitLab account from the account menu, as GitHub's sign-in
//! does: GitLab issues a code, the user approves it in the browser, and the
//! token GitLab grants is kept for the instance. Requests and polls run on
//! the background executor; the waits between polls are timers, so nothing
//! blocks the UI thread.

use super::no_store;
use crate::HerdrWindow;
use crate::i18n::fl;
use gpui::{prelude::*, *};
use herdr_orchestrator::gitlab::{
    self, Api, account, cli_hosts,
    oauth::{self, Poll},
    saved::{self, Kept},
};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};

/// GitLab asks for polls further apart by this much each time.
const SLOW_DOWN: Duration = Duration::from_secs(5);

/// An account being connected.
pub(crate) struct Connecting {
    pub(crate) host: String,
    /// The code to approve on GitLab, once GitLab issued it.
    pub(crate) code: Option<String>,
    /// GitLab's approval page, on the instance itself.
    pub(super) open: Option<url::Url>,
    pub(super) copied: bool,
    /// Asks GitLab, waits, and polls; dropping it stops the flow.
    _task: Task<()>,
}

#[cfg(test)]
impl Connecting {
    /// A flow showing `code`, with no work behind it.
    pub(crate) fn fixture(host: &str, code: &str) -> Self {
        Self {
            host: host.into(),
            code: Some(code.into()),
            open: url::Url::parse(&format!("https://{host}/oauth/device")).ok(),
            copied: false,
            _task: Task::ready(()),
        }
    }
}

impl HerdrWindow {
    /// Starts connecting an account on `host` with its OAuth application.
    pub(super) fn connect_gitlab(&mut self, host: String, cx: &mut Context<Self>) {
        let Some(store) = self.gitlab_store() else {
            self.menu.gitlab.message = Some(no_store());
            cx.notify();
            return;
        };
        let Some(application) = self.config.gitlab.application(&host).map(str::to_owned) else {
            return;
        };
        self.menu.gitlab.message = None;
        // The account is checked where the instance's remotes read it.
        let api = self.menu.gitlab.apis.get(&host).cloned();
        let task = cx.spawn({
            let host = host.clone();
            async move |this, cx| {
                let result = connect(&this, &host, api, application, store, cx).await;
                let _ = this.update(cx, |this, cx| {
                    this.menu.gitlab.connecting = None;
                    this.menu.gitlab.message = Some(match &result {
                        Ok(user) => fl!(
                            "gitlab-menu-connected",
                            host = host.as_str(),
                            user = user.to_string()
                        ),
                        Err(error) => error.to_string(),
                    });
                    cx.notify();
                });
                HerdrWindow::gitlab_changed(&this, result.is_ok(), cx);
            }
        });
        self.menu.gitlab.connecting = Some(Connecting {
            host,
            code: None,
            open: None,
            copied: false,
            _task: task,
        });
        cx.notify();
    }

    pub(super) fn cancel_gitlab_connect(&mut self, cx: &mut Context<Self>) {
        self.menu.gitlab.connecting = None;
        cx.notify();
    }

    pub(super) fn copy_gitlab_code(&mut self, cx: &mut Context<Self>) {
        let Some(connecting) = &mut self.menu.gitlab.connecting else {
            return;
        };
        if let Some(code) = &connecting.code {
            cx.write_to_clipboard(ClipboardItem::new_string(code.clone()));
            connecting.copied = true;
            cx.notify();
        }
    }

    /// Opens GitLab's approval page, which [`oauth::parse_device`] keeps to
    /// an HTTPS page on the instance.
    pub(super) fn open_gitlab_approval(&self, cx: &mut Context<Self>) {
        if let Some(open) = self
            .menu
            .gitlab
            .connecting
            .as_ref()
            .and_then(|connecting| connecting.open.as_ref())
        {
            cx.open_url(open.as_str());
        }
    }
}

/// Asks GitLab for a code, shows it, polls until the user approves it, then
/// checks the granted token and keeps it. Returns whom GitLab signs it in as.
async fn connect(
    this: &WeakEntity<HerdrWindow>,
    host: &str,
    api: Option<Api>,
    application: String,
    store: herdr_credentials::Store,
    cx: &mut AsyncApp,
) -> Result<String, gitlab::Error> {
    let device = cx
        .background_spawn({
            let (host, application) = (host.to_owned(), application.clone());
            async move { oauth::authorize(&host, &application) }
        })
        .await?;
    let deadline = Instant::now() + device.expires_in;
    let mut interval = device.interval;
    let _ = this.update(cx, |this, cx| {
        if let Some(connecting) = &mut this.menu.gitlab.connecting {
            connecting.code = Some(device.user_code.clone());
            connecting.open = Some(device.open.clone());
        }
        cx.notify();
    });
    let device = Arc::new(device);
    loop {
        cx.background_executor().timer(interval).await;
        if Instant::now() >= deadline {
            return Err(gitlab::Error::CodeExpired);
        }
        let poll = cx
            .background_spawn({
                let (host, application, device) =
                    (host.to_owned(), application.clone(), device.clone());
                async move { oauth::poll(&host, &application, &device, saved::now()) }
            })
            .await?;
        match poll {
            Poll::Pending => {}
            Poll::SlowDown => interval += SLOW_DOWN,
            Poll::Granted(pair) => {
                let host = host.to_owned();
                return cx
                    .background_spawn(async move {
                        // An instance no remote reads goes where glab sends
                        // its API.
                        let api =
                            api.unwrap_or_else(|| Api::of_instance(&host, &cli_hosts::read()));
                        let user = account::whoami(&api, &pair.access)?.username;
                        saved::save_kept(store, &host, Some(Kept::OAuth(pair)))?;
                        Ok(user)
                    })
                    .await;
            }
        }
    }
}
