//! With no Herdr 1 installed, the window's terminal is a herdr-core shell:
//! the user's running core, else the user's `herdr-core`, else the copy
//! bundled beside the app. The shell is named, so every launch comes back
//! to the same one instead of starting another. Only when herdr-core is
//! unavailable too does the window ask the user to install Herdr.

use super::{CoreTerminalView, Start, State, worker};
use crate::HerdrWindow;
use gpui::{prelude::*, *};
use herdr_backend::{Backend as _, Role, Size, Spawn, TerminalId};
use std::sync::mpsc;

/// The name of the shell the window shows in place of Herdr 1.
const SHELL: &str = "herdr-ui";

#[derive(Default)]
pub(crate) enum Fallback {
    #[default]
    Off,
    /// The shell starting, or found by name.
    Starting(mpsc::Receiver<Result<TerminalId, herdr_core_client::Error>>),
    Shown(Entity<CoreTerminalView>),
}

impl HerdrWindow {
    /// Runs every tick: while Herdr 1 is missing, connects and shows the
    /// shell; once Herdr 1 answers again, gives the window back to it.
    pub(super) fn poll_core_fallback(&mut self, cx: &mut Context<Self>) {
        if !self.live.missing_installation {
            if !matches!(self.core_terminals.fallback, Fallback::Off) {
                self.core_terminals.fallback = Fallback::Off;
                cx.notify();
            }
            return;
        }
        self.core_terminals.hub.connect(Start::IfMissing);
        match &self.core_terminals.fallback {
            Fallback::Off => self.start_fallback_shell(),
            Fallback::Starting(found) => match found.try_recv() {
                Ok(Ok(terminal)) => self.show_fallback_shell(terminal, cx),
                Ok(Err(error)) => {
                    tracing::warn!(%error, "Could not start the herdr-core shell");
                    self.core_terminals.fallback = Fallback::Off;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.core_terminals.fallback = Fallback::Off
                }
            },
            Fallback::Shown(_) => {}
        }
    }

    fn start_fallback_shell(&mut self) {
        let Some(backend) = self.core_terminals.hub.backend().cloned() else {
            return;
        };
        let (send, receive) = mpsc::sync_channel(1);
        let spawn = Spawn {
            name: Some(SHELL.to_owned()),
            ..Spawn::default()
        };
        let spawned = std::thread::Builder::new()
            .name("herdr-core-shell".into())
            .spawn(move || {
                let found = match backend.spawn(&spawn) {
                    Ok(info) => Ok(info.id),
                    // An earlier launch's shell still runs: show it again.
                    Err(herdr_core_client::Error::Refused {
                        kind: Some(kind), ..
                    }) if kind == "name_taken" => {
                        TerminalId::new(SHELL).map_err(herdr_core_client::Error::from)
                    }
                    Err(error) => Err(error),
                };
                let _ = send.send(found);
            });
        if spawned.is_ok() {
            self.core_terminals.fallback = Fallback::Starting(receive);
        }
    }

    fn show_fallback_shell(&mut self, terminal: TerminalId, cx: &mut Context<Self>) {
        let (Some(backend), Ok(size)) = (
            self.core_terminals.hub.backend().cloned(),
            Size::new(80, 24),
        ) else {
            self.core_terminals.fallback = Fallback::Off;
            return;
        };
        let link = match worker::spawn(backend, terminal.clone(), size, Role::Active) {
            Ok(link) => link,
            Err(error) => {
                tracing::warn!(%error, "Could not start a herdr-core terminal worker");
                self.core_terminals.fallback = Fallback::Off;
                return;
            }
        };
        let (look, painter) = (self.core_look(), self.painter.clone());
        let view = cx.new(|cx| CoreTerminalView::new(terminal, link, look, painter, false, cx));
        self.core_terminals.fallback = Fallback::Shown(view);
        cx.notify();
    }

    /// The shell shown in place of Herdr 1's terminal, while it is.
    pub(crate) fn core_fallback_view(&self) -> Option<Entity<CoreTerminalView>> {
        match &self.core_terminals.fallback {
            Fallback::Shown(view) => Some(view.clone()),
            _ => None,
        }
    }

    /// Whether the window should still ask the user to install Herdr: only
    /// once herdr-core is known to be unavailable as well.
    pub(crate) fn needs_herdr_install(&self) -> bool {
        matches!(self.core_terminals.hub.state(), State::Unavailable(_))
    }
}
