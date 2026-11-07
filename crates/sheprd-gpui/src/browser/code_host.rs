//! Which VS Code server serves the focused workspace's VS Code tab: the one
//! on this computer, or, for a workspace on an SSH host while the app starts
//! VS Code, that host's own (see [`crate::code_server::host`]), which opens
//! the workspace's folder there. Each server keeps its own address, answer,
//! and pages: moving between workspaces on different hosts disturbs neither.
use super::{Scope, code::CodeServer, view::scope};
use crate::{
    SheprdWindow,
    code_server::{Hosts, Launcher, Startup},
};
use gpui::App;
use sheprd_client::ConnectTarget;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum CodeHost {
    /// The server on this computer, or the configured address.
    Local,
    /// The server the app runs on this SSH target.
    Ssh(String),
}

impl SheprdWindow {
    /// The server for the focused workspace's VS Code tab.
    pub(crate) fn code_host(&self, cx: &App) -> CodeHost {
        match self
            .endpoints
            .get(self.selected_endpoint)
            .map(|e| &e.connection.target)
        {
            Some(ConnectTarget::Ssh { target, .. })
                if Hosts::startup(cx, &self.config.code, target).is_some() =>
            {
                CodeHost::Ssh(target.clone())
            }
            _ => CodeHost::Local,
        }
    }

    /// Where `host`'s server is, or why it is not there yet.
    pub(crate) fn code_startup(&self, host: &CodeHost, cx: &App) -> Startup {
        let code = &self.config.code;
        match host {
            CodeHost::Ssh(target) => Hosts::startup(cx, code, target),
            CodeHost::Local => None,
        }
        .unwrap_or_else(|| Launcher::startup(cx, code))
    }

    /// Stops the servers the config no longer starts, and starts `host`'s
    /// when `wanted`, as when a page first needs it.
    pub(super) fn sync_code_servers(&self, host: &CodeHost, wanted: bool, cx: &mut App) {
        let code = &self.config.code;
        Launcher::sync(cx, code);
        Hosts::sync(cx, code);
        if wanted {
            match host {
                CodeHost::Local => Launcher::want(cx, code),
                CodeHost::Ssh(target) => Hosts::want(cx, code, target),
            }
        }
        Launcher::refresh(cx);
        Hosts::refresh(cx);
    }

    /// Whether `host`'s server, which the app started, runs right now.
    pub(super) fn code_server_alive(host: &CodeHost, cx: &App) -> bool {
        match host {
            CodeHost::Local => Launcher::alive(cx),
            CodeHost::Ssh(target) => Hosts::alive(cx, target),
        }
    }

    /// What the window knows of `host`'s server.
    pub(super) fn code_server(&self, host: &CodeHost) -> Option<&CodeServer> {
        match host {
            CodeHost::Local => Some(&self.browser.code_server),
            CodeHost::Ssh(target) => self.browser.host_code_servers.get(target),
        }
    }

    /// The same, to change. A host's starts out asked as the local one is,
    /// which tests answer for.
    pub(super) fn code_server_mut(&mut self, host: &CodeHost) -> &mut CodeServer {
        let browser = &mut self.browser;
        match host {
            CodeHost::Local => &mut browser.code_server,
            CodeHost::Ssh(target) => {
                let (probe, started_probe) =
                    (browser.code_server.probe, browser.code_server.started_probe);
                browser
                    .host_code_servers
                    .entry(target.clone())
                    .or_insert_with(|| CodeServer::asked_by(probe, started_probe))
            }
        }
    }

    /// The scopes of this window's workspaces that `host` serves. The local
    /// server serves every daemon on this computer, and this window's hosts
    /// that have no server of their own; a host serves its own workspaces.
    /// A scope this window does not show is left to the window that does.
    pub(super) fn code_scopes(&self, host: &CodeHost, cx: &App) -> impl Fn(&Scope) -> bool + use<> {
        let code = &self.config.code;
        let mine: Vec<(Scope, CodeHost)> = self
            .endpoints
            .iter()
            .map(|endpoint| {
                let served = match &endpoint.connection.target {
                    ConnectTarget::Ssh { target, .. }
                        if Hosts::startup(cx, code, target).is_some() =>
                    {
                        CodeHost::Ssh(target.clone())
                    }
                    _ => CodeHost::Local,
                };
                (scope(endpoint), served)
            })
            .collect();
        let host = host.clone();
        move |tab: &Scope| match mine.iter().find(|(scope, _)| scope == tab) {
            Some((_, served)) => *served == host,
            None => host == CodeHost::Local && tab.is_local(),
        }
    }

    /// Closes the VS Code pages `host` serves, so each reopens on its next
    /// address. Only builds that show pages have any.
    #[cfg(any(target_os = "macos", windows))]
    pub(super) fn close_code_pages(&mut self, host: &CodeHost, cx: &App) {
        let serves = self.code_scopes(host, cx);
        self.close_editor_pages_in(super::Editor::VsCode, serves, cx);
    }
}
