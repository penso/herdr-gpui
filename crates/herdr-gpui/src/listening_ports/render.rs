//! Drawing a workspace's listening ports: a globe and `:3000 :5173` after
//! it, each number opening its page in a browser tab of that workspace.

use super::{
    Link, Port,
    tunnel::{self, Key},
};
use crate::{
    browser::WebUrl,
    config::Theme,
    i18n::{Localize, fl},
    window::{Flash, HerdrWindow},
};
use gpui::{prelude::*, *};

const GAP: f32 = 6.;

impl HerdrWindow {
    /// The focused workspace's ports in the status bar; nothing while hidden
    /// or while it listens on none.
    pub(crate) fn render_listening_ports(&self, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let (endpoint, workspace, listed) = self.focused_listening_ports()?;
        Some(
            div()
                .id("listening-ports")
                .debug_selector(|| "listening-ports".into())
                .role(Role::Group)
                .aria_label(fl!("listening-ports-label"))
                .flex()
                .flex_none()
                .items_center()
                .h_full()
                .px(px(6.))
                .child(chips(
                    listed,
                    (endpoint, workspace),
                    &self.tunnels,
                    &self.theme,
                    14.,
                    cx,
                )),
        )
    }

    /// The focused workspace's ports, with the endpoint and workspace a
    /// click opens them in; None while hidden or while it listens on none.
    pub(crate) fn focused_listening_ports(&self) -> Option<(&str, &str, super::Listed<'_>)> {
        if !self.config.show_listening_ports {
            return None;
        }
        let endpoint = self.endpoints.get(self.selected_endpoint)?;
        let workspace = self
            .live
            .snapshot
            .as_ref()?
            .focused_workspace_id
            .as_deref()?;
        let daemon = super::Daemon::of(&endpoint.connection.target)?;
        let listed = self.listening_ports.get(&daemon, workspace)?;
        Some((endpoint.id.as_str(), workspace, listed))
    }

    /// Opens a port's page in a browser tab of `workspace`, through a tunnel
    /// when the port is on a remote host.
    pub(crate) fn open_port_link(
        &mut self,
        endpoint: &str,
        workspace: &str,
        link: &Link,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<crate::browser::PageOpened> {
        match link {
            Link::Page(url) => {
                Some(self.open_workspace_page(endpoint, workspace, url.clone(), window, cx))
            }
            // Opened once the tunnel is up, later.
            Link::Tunnel(key) => {
                self.open_tunneled_page(endpoint, workspace, key.clone(), window, cx);
                None
            }
        }
    }
}

/// A globe and one clickable number per port. `place` is the endpoint and
/// workspace whose browser tab a click opens. A port reached through an open
/// SSH tunnel also gets a small close button that shuts that tunnel.
pub(crate) fn chips(
    listed: super::Listed<'_>,
    (endpoint, workspace): (&str, &str),
    tunnels: &super::Tunnels,
    theme: &Theme,
    glyph: f32,
    cx: &mut Context<HerdrWindow>,
) -> Div {
    let (muted, foreground, surface) = (theme.muted, theme.foreground, theme.surface);
    div()
        .flex()
        .items_center()
        .gap(px(GAP))
        .overflow_hidden()
        .child(
            svg()
                .path("icons/globe.svg")
                .size(px(glyph))
                .flex_none()
                .text_color(rgb(muted)),
        )
        .children(listed.ports.iter().map(|port| {
            let link = port.link(listed.origin);
            let hint = SharedString::from(hint(port, link.as_ref()));
            let id = SharedString::from(format!("port-{endpoint}-{workspace}-{}", port.number));
            let chip = div()
                .id(id.clone())
                .debug_selector(|| id.to_string())
                .flex_none()
                .text_color(rgb(muted))
                .child(format!(":{}", port.number))
                .tooltip(move |_, cx| {
                    cx.new(|_| crate::usage::Hint {
                        text: hint.clone(),
                        foreground,
                        surface,
                    })
                    .into()
                });
            let Some(link) = link else {
                return div().flex_none().child(chip);
            };
            let close = match &link {
                Link::Tunnel(key) if tunnels.is_open(key) => Some(close_button(
                    format!("port-close-{endpoint}-{workspace}-{}", key.port).into(),
                    key,
                    glyph,
                    theme,
                    cx,
                )),
                _ => None,
            };
            let (endpoint, workspace) = (endpoint.to_owned(), workspace.to_owned());
            let chip = chip
                .cursor_pointer()
                .hover(|style| style.text_color(rgb(foreground)).underline())
                .on_click(cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    this.open_port_link(&endpoint, &workspace, &link, window, cx);
                }));
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(2.))
                .child(chip)
                .children(close)
        }))
}

/// The small `x` after a port whose SSH tunnel is open; clicking it closes
/// the tunnel, which frees its local port. `id` is unique per place it shows.
pub(crate) fn close_button(
    id: SharedString,
    key: &Key,
    glyph: f32,
    theme: &Theme,
    cx: &mut Context<HerdrWindow>,
) -> Stateful<Div> {
    let (muted, foreground, surface) = (theme.muted, theme.foreground, theme.surface);
    let hint = SharedString::from(fl!(
        "listening-ports-close-tunnel",
        target = key.target.clone(),
        port = key.port.to_string()
    ));
    let key = key.clone();
    div()
        .id(id.clone())
        .debug_selector(|| id.to_string())
        .flex_none()
        .cursor_pointer()
        .child(
            svg()
                .path("icons/close.svg")
                .size(px((glyph * 0.8).round()))
                .text_color(rgb(muted)),
        )
        .hover(|style| style.bg(rgb(surface)))
        .tooltip(move |_, cx| {
            cx.new(|_| crate::usage::Hint {
                text: hint.clone(),
                foreground,
                surface,
            })
            .into()
        })
        .on_click(cx.listener(move |this, _, _, cx| {
            cx.stop_propagation();
            if this.tunnels.close(&key) {
                this.show_flash(
                    Flash::success(fl!(
                        "listening-ports-tunnel-closed",
                        port = key.port.to_string()
                    )),
                    cx,
                );
                cx.notify();
            }
        }))
}

/// `node listening on *:3000` over what clicking does.
pub(super) fn hint(port: &Port, link: Option<&Link>) -> String {
    let listening = match port.process.as_str() {
        "" => fl!(
            "listening-ports-unnamed-listening",
            address = port.address()
        ),
        process => fl!(
            "listening-ports-listening",
            process = process,
            address = port.address()
        ),
    };
    let action = match link {
        Some(Link::Page(url)) => fl!("listening-ports-open", url = url.as_str()),
        Some(Link::Tunnel(key)) => fl!("listening-ports-open-tunnel", target = key.target.clone()),
        None => fl!("listening-ports-unreachable"),
    };
    format!("{listening}\n{action}")
}

impl HerdrWindow {
    /// Opens a remote port through an SSH tunnel: an existing
    /// one at once, otherwise a new one started on the background executor,
    /// since SSH may take seconds to connect. The page opens once the tunnel
    /// accepts; a window closed meanwhile drops, and so kills, the tunnel.
    pub(crate) fn open_tunneled_page(
        &mut self,
        endpoint: &str,
        workspace: &str,
        key: Key,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(local) = self.tunnels.local(&key) {
            self.open_local_port(endpoint, workspace, local, window, cx);
            return;
        }
        let Some(preferred) = self.tunnels.begin(&key) else {
            self.show_flash(Flash::warning(fl!("listening-ports-tunnel-opening")), cx);
            return;
        };
        let opening = key.clone();
        let attempt = self.tunnels.attempt(&key).unwrap_or_default();
        let task = cx
            .background_executor()
            .spawn(async move { tunnel::open(&opening, preferred, &attempt) });
        let (endpoint, workspace) = (endpoint.to_owned(), workspace.to_owned());
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(tunnel) => {
                    if let Some(local) = this.tunnels.finish(key, tunnel) {
                        this.open_local_port(&endpoint, &workspace, local, window, cx);
                    }
                }
                Err(error) => {
                    // Not worth a complaint once the user closed the port.
                    if this.tunnels.fail(&key) {
                        // The display boundary for a failed tunnel.
                        this.show_flash(
                            Flash::warning(fl!(
                                "listening-ports-open-failed",
                                port = key.port.to_string(),
                                error = error.localize()
                            )),
                            cx,
                        );
                    }
                }
            });
        })
        .detach();
    }

    fn open_local_port(
        &mut self,
        endpoint: &str,
        workspace: &str,
        local: u16,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match WebUrl::try_from(format!("http://127.0.0.1:{local}/").as_str()) {
            Ok(url) => {
                self.open_workspace_page(endpoint, workspace, url, window, cx);
            }
            Err(error) => self.show_flash(Flash::warning(error.to_string()), cx),
        }
    }
}
