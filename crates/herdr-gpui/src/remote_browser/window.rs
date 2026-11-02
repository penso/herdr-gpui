//! A window's side of remote requests: find new ones in its endpoints'
//! snapshots, open their pages, and wait for the forwards of pages on a
//! host's loopback.
use super::{Action, Request, Seen, SeenKey};
use crate::{
    HerdrWindow,
    browser::{Location, Store, WebUrl},
    i18n::{Localize, fl},
    port_forward::State,
    usage::Host,
    window::Flash,
};
use gpui::{Context, Window};
use std::{
    num::NonZeroU16,
    time::{Duration, Instant},
};

/// How long a page waits for its forward, as `ports open` does.
const FORWARD_WAIT: Duration = Duration::from_secs(60);
/// Pages waiting at once; each holds a forward, which is bounded too.
const MAX_FORWARDING: usize = 8;

/// A page on a host's loopback, waiting for this window's forward of its
/// port to listen.
pub(crate) struct Forwarding {
    pub(super) endpoint: String,
    pub(super) workspace: String,
    pub(super) pane: Option<String>,
    pub(super) url: WebUrl,
    pub(super) target: String,
    pub(super) port: NonZeroU16,
    pub(super) focus: bool,
    pub(super) until: Instant,
}

impl HerdrWindow {
    /// Serves requests new in this window's snapshots, then opens pages
    /// whose forwards now listen. Runs every tick.
    pub(crate) fn poll_remote_browser(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut requests = Vec::new();
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            let live = if index == self.selected_endpoint {
                &self.live
            } else {
                &endpoint.live
            };
            let Some(snapshot) = live.snapshot.as_ref() else {
                continue;
            };
            for workspace in &snapshot.workspaces {
                if !workspace
                    .tokens
                    .iter()
                    .any(|(name, _)| super::is_request_token(name))
                {
                    continue;
                }
                for request in Request::all(&workspace.tokens) {
                    requests.push((index, workspace.workspace_id.clone(), request));
                }
            }
        }
        let window_id = window.window_handle().window_id().as_u64();
        for (index, workspace, request) in requests {
            let key = SeenKey {
                scope: crate::browser::scope(&self.endpoints[index]),
                workspace: workspace.clone(),
                id: request.id.clone(),
                window: matches!(request.action, Action::Reload).then_some(window_id),
            };
            if cx.default_global::<Seen>().first(key) {
                self.serve_remote_request(index, workspace, request, window, cx);
            }
        }
        self.poll_remote_forwards(window, cx);
        self.install_remote_agent_tools(cx);
    }

    fn serve_remote_request(
        &mut self,
        index: usize,
        workspace: String,
        request: Request,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Request { pane, action, .. } = request;
        let (address, focus) = match action {
            Action::Reload => {
                let Some(pane) = pane else {
                    return;
                };
                let scope = crate::browser::scope(&self.endpoints[index]);
                let tabs: Vec<_> = cx
                    .try_global::<Store>()
                    .map(|store| {
                        store
                            .opened_by(&pane)
                            .filter(|tab| tab.scope == scope)
                            .map(|tab| tab.id)
                            .collect()
                    })
                    .unwrap_or_default();
                self.reload_browser_tabs(&tabs);
                return;
            }
            Action::Open { address, focus } => (address, focus),
        };
        let label = self.endpoints[index].label.clone();
        let Ok(url) = WebUrl::from_typed(&address) else {
            self.show_flash(
                Flash::warning(fl!("remote-browser-bad-address", device = label)),
                cx,
            );
            return;
        };
        let local = Host::of(&self.endpoints[index].connection.target)
            .is_some_and(|host| matches!(host, Host::Local | Host::Wsl(_)));
        let Some(port) = url.loopback_port().filter(|_| !local) else {
            self.open_remote_page(index, &workspace, pane.as_deref(), url, focus, window, cx);
            return;
        };
        let Some(target) = self.endpoints[index]
            .saved_ssh()
            .map(|(target, _)| target.to_owned())
        else {
            self.show_flash(
                Flash::warning(fl!(
                    "remote-browser-needs-ssh",
                    device = label,
                    address = url.address()
                )),
                cx,
            );
            return;
        };
        if self.remote_browser.len() >= MAX_FORWARDING {
            self.show_flash(Flash::warning(fl!("remote-browser-too-many-forwards")), cx);
            return;
        }
        let running = self
            .port_forwards
            .for_host(&target)
            .find(|forward| forward.remote_port() == port)
            .is_some_and(|forward| !matches!(forward.state(), State::Ended(_)));
        if !running && let Err(error) = self.port_forwards.start(&target, port) {
            self.show_flash(Flash::warning(error.localize()), cx);
            return;
        }
        self.remote_browser.push(Forwarding {
            endpoint: self.endpoints[index].id.clone(),
            workspace,
            pane,
            url,
            target,
            port,
            focus,
            until: Instant::now() + FORWARD_WAIT,
        });
        cx.notify();
    }

    /// Opens pages whose forwards listen, and drops the ones whose forwards
    /// ended, ran out of time, or whose host left the window.
    fn poll_remote_forwards(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.remote_browser.is_empty() {
            return;
        }
        let now = Instant::now();
        for page in std::mem::take(&mut self.remote_browser) {
            let Some(index) = self.endpoints.iter().position(|e| e.id == page.endpoint) else {
                continue;
            };
            let state = self
                .port_forwards
                .for_host(&page.target)
                .find(|forward| forward.remote_port() == page.port)
                .map(|forward| forward.state().clone());
            match state {
                Some(State::Listening { local_port }) => {
                    let Some(url) = page.url.at_local_port(local_port) else {
                        continue;
                    };
                    let pane = page.pane.as_deref();
                    self.open_remote_page(
                        index,
                        &page.workspace,
                        pane,
                        url,
                        page.focus,
                        window,
                        cx,
                    );
                }
                // The forward's own notice says why it ended.
                Some(State::Ended(_)) | None => {}
                Some(State::Starting) if now >= page.until => {
                    self.show_flash(
                        Flash::warning(fl!(
                            "remote-browser-forward-timeout",
                            port = page.port.to_string()
                        )),
                        cx,
                    );
                }
                Some(State::Starting) => self.remote_browser.push(page),
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn open_remote_page(
        &mut self,
        index: usize,
        workspace: &str,
        pane: Option<&str>,
        url: WebUrl,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !crate::browser::EMBEDDED {
            cx.open_url(url.as_str());
            return;
        }
        let location = Location::Web { url };
        if self
            .open_agent_tab(index, workspace, pane, &location, focus, window, cx)
            .is_none()
        {
            self.show_flash(Flash::warning(fl!("remote-browser-too-many-tabs")), cx);
        }
    }
}
