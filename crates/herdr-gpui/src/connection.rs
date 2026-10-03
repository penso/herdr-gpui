use crate::{
    config::Theme,
    state::{ConnectionStatus, LiveState},
    terminal::InputTarget,
};
use herdr_client::{
    ClientEvent, ClientHandle, ConnectOptions, ConnectTarget, HostTheme, Method,
    connect_with_connector,
    protocol::{ClientHostAppearance, ClientHostColor, ClientPaneInputEvent},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub(crate) struct ConnectionBridge {
    pub target: ConnectTarget,
    pub handle: Option<ClientHandle>,
    pub inbox: Arc<Mutex<LiveState>>,
    pub drained: Arc<AtomicBool>,
    pub integrations: Arc<Mutex<IntegrationInbox>>,
    pub(crate) links: Arc<Mutex<crate::links::LinkInbox>>,
    /// Scrollback answers, fenced with the connection like the main inbox.
    pub scrollback: Arc<Mutex<crate::scrollback::Inbox>>,
    sound_cancel: Arc<AtomicBool>,
}

/// One integration operation, independent of the modal dialog response slot.
#[derive(Default)]
pub(crate) struct IntegrationInbox {
    pub list: bool,
    pub install: bool,
    pub pending: Option<(String, Option<crate::Result<serde_json::Value>>)>,
}

impl IntegrationInbox {
    fn apply(&mut self, event: ClientEvent) -> Option<ClientEvent> {
        match &event {
            ClientEvent::Connected(welcome) => {
                self.list = Method::IntegrationList.advertised_in(&welcome.methods);
                self.install = Method::IntegrationInstall.advertised_in(&welcome.methods);
            }
            ClientEvent::Disconnected { .. } => {
                self.list = false;
                self.install = false;
                if let Some((_, result)) = &mut self.pending {
                    *result = Some(Err(crate::Error::NotConnected));
                }
            }
            _ => {}
        }
        let Some((id, result)) = &mut self.pending else {
            return Some(event);
        };
        match event {
            ClientEvent::Response {
                request_id,
                response,
            } if request_id == *id => {
                *result = Some(Ok(response));
                None
            }
            ClientEvent::CommandRejected {
                request_id: Some(request_id),
                reason,
            } if request_id == *id => {
                *result = Some(Err(crate::Error::Client(reason)));
                None
            }
            event => Some(event),
        }
    }
}

/// What the daemon is told about this client's terminal: the colors cells are
/// painted with, and the system appearance rather than the theme's lightness,
/// so Herdr's light and dark theme overrides follow the OS as they would in a
/// terminal that reports its color scheme.
pub(crate) fn host_theme(theme: &Theme, light: bool) -> HostTheme {
    let rgb = |color: u32| {
        let [_, r, g, b] = color.to_be_bytes();
        ClientHostColor { r, g, b }
    };
    HostTheme {
        foreground: rgb(theme.foreground),
        background: rgb(theme.background),
        palette: theme.palette.map(rgb),
        appearance: if light {
            ClientHostAppearance::Light
        } else {
            ClientHostAppearance::Dark
        },
    }
}

impl ConnectionBridge {
    pub fn new(target: ConnectTarget) -> Self {
        let state = LiveState::default();
        Self {
            target,
            handle: None,
            sound_cancel: state.sound_connection_cancel.clone(),
            inbox: Arc::new(Mutex::new(state)),
            drained: Arc::new(AtomicBool::new(true)),
            integrations: Arc::default(),
            links: Arc::default(),
            scrollback: Arc::default(),
        }
    }

    fn reset(&mut self, status: ConnectionStatus, active: bool) {
        // Retire audio even when the event reducer holds the inbox. Boot-scoped
        // cancellation alone cannot be reached without that lock.
        self.sound_cancel.store(true, Ordering::Release);
        if let Ok(mut state) = self.inbox.try_lock() {
            state.cancel_sounds();
        }
        if let Some(handle) = self.handle.take() {
            handle.disconnect();
        }
        let mut state = LiveState::default();
        state.status = status;
        state.set_outer_focus(active);
        self.sound_cancel = state.sound_connection_cancel.clone();
        // Old readers and deferred paint acknowledgements retain only the old inbox.
        self.inbox = Arc::new(Mutex::new(state));
        self.drained = Arc::new(AtomicBool::new(true));
        self.integrations = Arc::default();
        self.links = Arc::default();
        self.scrollback = Arc::default();
    }

    pub fn detach(&mut self, active: bool) {
        tracing::debug!("Connection bridge detaching");
        self.reset(ConnectionStatus::Detached, active);
    }

    pub fn reconnect(&mut self, options: ConnectOptions, active: bool, surface_active: bool) {
        tracing::debug!("Connection bridge reconnecting");
        self.reset(ConnectionStatus::Connecting, active);
        self.start(options, surface_active, |events| {
            std::thread::Builder::new()
                .name("herdr-gui-events".into())
                .spawn(events)
                .map(|_| ())
        });
    }

    fn start(
        &mut self,
        options: ConnectOptions,
        surface_active: bool,
        spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> std::io::Result<()>,
    ) {
        self.drained = Arc::new(AtomicBool::new(false));
        let target = self.target.clone();
        let startup_inbox = self.inbox.clone();
        let result =
            connect_with_connector(target, options, surface_active, move |target, stop| {
                if matches!(target, ConnectTarget::Coder { .. }) {
                    return crate::coder::connect(target, stop);
                }
                let result = crate::daemon::connect(target, stop, || {
                    tracing::debug!("Connection bridge starting local daemon");
                    if let Ok(mut state) = startup_inbox.lock()
                        && state.status == ConnectionStatus::Connecting
                    {
                        state.daemon_starting();
                    }
                })
                .map(|(stream, local)| {
                    if let Ok(mut state) = startup_inbox.lock() {
                        state.local_daemon_peer = local;
                        state.dirty = true;
                    }
                    stream
                });
                if let Err(error) = &result {
                    let category = if crate::daemon::is_missing_installation(error) {
                        "missing_installation"
                    } else {
                        "daemon_connect"
                    };
                    tracing::debug!(category, error_kind = ?error.kind(), "Connection bridge connector failed");
                }
                if result
                    .as_ref()
                    .is_err_and(crate::daemon::is_missing_installation)
                    && let Ok(mut state) = startup_inbox.lock()
                    && state.status == ConnectionStatus::StartingDaemon
                {
                    state.missing_installation = true;
                    state.dirty = true;
                }
                result.map(herdr_client::Transport::from)
            })
            .map_err(crate::Error::from)
            .and_then(|client| {
                self.handle = Some(client.handle);
                let inbox = self.inbox.clone();
                let drained = self.drained.clone();
                let integrations = self.integrations.clone();
                let links = self.links.clone();
                let scrollback = self.scrollback.clone();
                // Drain ordered events even while GPUI is busy; retain only coherent state.
                spawn(Box::new(move || {
                    while let Ok(event) = client.events.recv() {
                        match &event {
                            ClientEvent::Connected(_) => tracing::debug!("Connection bridge connected"),
                            ClientEvent::Disconnected { .. } => tracing::debug!(category = "transport_disconnected", "Connection bridge disconnected"),
                            _ => {}
                        }
                        let event = match integrations.lock() {
                            Ok(mut integrations) => integrations.apply(event),
                            Err(_) => Some(event),
                        };
                        let event = event.and_then(|event| match scrollback.lock() {
                            Ok(mut scrollback) => scrollback.apply(event),
                            Err(_) => Some(event),
                        });
                        let event = event.and_then(|event| match links.lock() {
                            Ok(mut links) => links.apply(event),
                            Err(_) => Some(event),
                        });
                        if let Ok(mut state) = inbox.lock() {
                            match event {
                                Some(event) => state.apply(event),
                                // A link answer changes nothing the live state
                                // holds, but the window polls only when it is dirty.
                                None => state.dirty = true,
                            }
                        }
                    }
                    drained.store(true, Ordering::Release);
                    tracing::debug!("Connection bridge event reader drained");
                }))
                .map_err(crate::Error::from)
            });
        if let Err(error) = result {
            tracing::warn!(
                category = "bridge_startup",
                "Connection bridge startup failed"
            );
            self.drained.store(true, Ordering::Release);
            if let Some(handle) = self.handle.take() {
                handle.disconnect();
            }
            if let Ok(mut state) = self.inbox.lock() {
                state.apply(ClientEvent::Disconnected {
                    reason: error.to_string(),
                });
            }
        }
    }

    /// Cheap enough to call every display frame: never blocks the UI thread.
    pub fn has_update(&self) -> bool {
        self.inbox.try_lock().is_ok_and(|state| state.dirty)
    }

    pub fn take_update(&self) -> Option<LiveState> {
        let mut state = self.inbox.try_lock().ok()?;
        if !state.dirty {
            return None;
        }
        state.dirty = false;
        // Deliver the single response once rather than cloning a potentially large
        // checkout list into every subsequent surface update.
        let response = state
            .dialog_response
            .as_mut()
            .and_then(|(_, result)| result.take());
        let notifications = std::mem::take(&mut state.notifications);
        let notifications_lost = std::mem::take(&mut state.notifications_lost);
        let sounds = std::mem::take(&mut state.sound_events);
        let reload_sound = std::mem::take(&mut state.reload_sound);
        let clipboard_writes = std::mem::take(&mut state.clipboard_writes);
        let bells = std::mem::take(&mut state.bells);
        let mut update = state.clone();
        update.settings_reload = false;
        update.notifications = notifications;
        update.notifications_lost = notifications_lost;
        update.sound_events = sounds;
        update.reload_sound = reload_sound;
        update.clipboard_writes = clipboard_writes;
        update.bells = bells;
        if let Some((_, result)) = &mut update.dialog_response {
            *result = response;
        }
        Some(update)
    }

    /// Drain only when the background settings loader can accept a reload.
    /// Inbox contention leaves the coalesced request pending for the next poll.
    pub(crate) fn take_settings_reload(&self) -> bool {
        self.inbox
            .try_lock()
            .map(|mut state| std::mem::take(&mut state.settings_reload))
            .unwrap_or(false)
    }

    pub fn request_dialog(
        &self,
        boot_id: &str,
        method: Method,
        params: serde_json::Value,
    ) -> crate::Result<String> {
        // Register while holding the mailbox so even an immediate rejection is retained.
        let mut state = self
            .inbox
            .try_lock()
            .map_err(|_| crate::Error::ConnectionBusy)?;
        let id = self
            .handle
            .as_ref()
            .ok_or(crate::Error::NotConnected)?
            .request(boot_id, method, params)?;
        state.dialog_response = Some((id.clone(), None));
        Ok(id)
    }

    pub fn request_integration(
        &self,
        boot_id: &str,
        method: Method,
        params: serde_json::Value,
    ) -> crate::Result<String> {
        // Registration and event delivery share this lock, including immediate rejection.
        let mut inbox = self
            .integrations
            .try_lock()
            .map_err(|_| crate::Error::ConnectionBusy)?;
        if inbox.pending.is_some() {
            return Err(crate::Error::ConnectionBusy);
        }
        let supported = match method {
            Method::IntegrationList => inbox.list,
            Method::IntegrationInstall => inbox.install,
            _ => false,
        };
        if !supported {
            return Err(herdr_client::Error::UnsupportedMethod.into());
        }
        let id = self
            .handle
            .as_ref()
            .ok_or(crate::Error::NotConnected)?
            .request(boot_id, method, params)?;
        inbox.pending = Some((id.clone(), None));
        Ok(id)
    }

    /// Queues a link request without waiting on the event reader: a busy
    /// mailbox is reported as such, and the caller tries again later.
    pub(crate) fn request_link(
        &self,
        request: crate::links::LinkRequest,
        cell: &crate::links::LinkCell,
    ) -> crate::Result<String> {
        let mut links = self
            .links
            .try_lock()
            .map_err(|_| crate::Error::ConnectionBusy)?;
        links.request(
            self.handle.as_ref().ok_or(crate::Error::NotConnected)?,
            request,
            cell,
        )
    }

    /// Report `theme` once this connection has a snapshot to address it to.
    /// The handle skips repeats and starts over on every new connection, so
    /// this is safe to call whenever the theme or appearance may have changed.
    pub fn sync_host_theme(&self, live: &LiveState, theme: &HostTheme) {
        let (Some(handle), Some(snapshot)) = (&self.handle, &live.snapshot) else {
            return;
        };
        if !live.status.is_connected() {
            return;
        }
        // A full queue leaves nothing recorded; the next sync resends it all.
        if let Err(error) = handle.set_host_theme(&snapshot.boot_id, theme) {
            tracing::debug!(%error, "Connection bridge host theme not queued");
        }
    }

    pub fn send_input(
        handle: &ClientHandle,
        boot_id: &str,
        target: &InputTarget,
        event: ClientPaneInputEvent,
    ) -> Result<(), herdr_client::SendError> {
        match target {
            InputTarget::Pane(id) => handle.send_input(boot_id, id, [event]),
            InputTarget::Popup(id) => handle.send_popup_input(boot_id, id, [event]),
        }
    }
}

impl Drop for ConnectionBridge {
    fn drop(&mut self) {
        self.sound_cancel.store(true, Ordering::Release);
        if let Ok(mut state) = self.inbox.try_lock() {
            state.cancel_sounds();
        }
        // Detach this client only; never kill a daemon or PTY.
        if let Some(handle) = &self.handle {
            tracing::debug!("Connection bridge dropping client");
            handle.disconnect();
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn bridge() -> ConnectionBridge {
        ConnectionBridge::new(ConnectTarget::Socket("/unused-connection-test.sock".into()))
    }

    #[test]
    fn host_theme_paints_cell_colors_and_follows_system_appearance() {
        let theme = Theme {
            foreground: 0xabcdef,
            background: 0x010203,
            ..Theme::default()
        };
        let dark = host_theme(&theme, false);
        assert_eq!(dark.appearance, ClientHostAppearance::Dark);
        assert_eq!(
            dark.foreground,
            ClientHostColor {
                r: 0xab,
                g: 0xcd,
                b: 0xef
            }
        );
        assert_eq!(dark.background, ClientHostColor { r: 1, g: 2, b: 3 });
        assert!(
            dark.palette
                .iter()
                .zip(theme.palette)
                .all(|(color, packed)| (u32::from(color.r) << 16
                    | u32::from(color.g) << 8
                    | u32::from(color.b))
                    == packed)
        );
        // A dark background under a light system still reports light.
        let light = host_theme(&theme, true);
        assert_eq!(light.appearance, ClientHostAppearance::Light);
        assert_eq!(light.palette, dark.palette);
    }

    #[test]
    fn settings_reload_is_coalesced_consumed_once_and_connection_fenced() {
        let mut bridge = bridge();
        for _ in 0..3 {
            bridge.inbox.lock().unwrap().apply(ClientEvent::Message(
                herdr_client::protocol::ServerMessage::ReloadSoundConfig,
            ));
        }
        let update = bridge.take_update().unwrap();
        assert!(!update.settings_reload);
        assert!(update.reload_sound);
        let guard = bridge.inbox.lock().unwrap();
        assert!(!bridge.take_settings_reload());
        drop(guard);
        assert!(bridge.take_settings_reload());
        assert!(!bridge.take_settings_reload());
        bridge.inbox.lock().unwrap().apply(ClientEvent::Message(
            herdr_client::protocol::ServerMessage::ReloadSoundConfig,
        ));
        assert!(bridge.take_settings_reload());
        assert!(bridge.take_update().unwrap().reload_sound);
        bridge.inbox.lock().unwrap().set_outer_focus(true);
        assert!(!bridge.take_update().unwrap().reload_sound);
        let old = bridge.inbox.clone();
        old.lock().unwrap().settings_reload = true;
        bridge.detach(false);
        assert!(!bridge.take_settings_reload());
        old.lock().unwrap().apply(ClientEvent::Message(
            herdr_client::protocol::ServerMessage::ReloadSoundConfig,
        ));
        assert!(!bridge.take_settings_reload());
        bridge.inbox.lock().unwrap().apply(ClientEvent::Message(
            herdr_client::protocol::ServerMessage::ReloadSoundConfig,
        ));
        bridge
            .inbox
            .lock()
            .unwrap()
            .apply(ClientEvent::Disconnected {
                reason: "closed".into(),
            });
        assert!(!bridge.take_settings_reload());
    }

    #[test]
    fn integration_responses_do_not_overwrite_dialog_responses() {
        let mut integrations = IntegrationInbox {
            pending: Some(("integration".into(), None)),
            ..Default::default()
        };
        let mut state = LiveState::default();
        state.dialog_response = Some(("dialog".into(), None));
        for (id, response) in [
            (
                "integration",
                serde_json::json!({"result":{"type":"integration_list"}}),
            ),
            (
                "dialog",
                serde_json::json!({"result":{"type":"worktree_list"}}),
            ),
        ] {
            if let Some(event) = integrations.apply(ClientEvent::Response {
                request_id: id.into(),
                response,
            }) {
                state.apply(event);
            }
        }
        assert_eq!(
            integrations.pending.unwrap().1.unwrap().unwrap()["result"]["type"],
            "integration_list"
        );
        assert_eq!(
            state.dialog_response.unwrap().1.unwrap().unwrap()["result"]["type"],
            "worktree_list"
        );
    }

    #[test]
    fn integration_rejections_are_correlated_and_disconnect_revokes_capabilities() {
        let mut inbox = IntegrationInbox {
            list: true,
            install: true,
            pending: Some(("install".into(), None)),
        };
        assert!(
            inbox
                .apply(ClientEvent::CommandRejected {
                    request_id: Some("other".into()),
                    reason: herdr_client::Error::CommandBoot,
                })
                .is_some()
        );
        assert!(inbox.pending.as_ref().unwrap().1.is_none());
        assert!(
            inbox
                .apply(ClientEvent::CommandRejected {
                    request_id: Some("install".into()),
                    reason: herdr_client::Error::CommandBoot,
                })
                .is_none()
        );
        assert!(matches!(
            inbox.pending.as_ref().unwrap().1,
            Some(Err(crate::Error::Client(herdr_client::Error::CommandBoot)))
        ));
        assert!(
            inbox
                .apply(ClientEvent::Disconnected {
                    reason: "closed".into()
                })
                .is_some()
        );
        assert!(!inbox.list && !inbox.install);
        assert!(matches!(
            inbox.pending.unwrap().1,
            Some(Err(crate::Error::NotConnected))
        ));
    }

    #[test]
    fn integration_mailbox_is_fenced_on_detach() {
        let mut bridge = bridge();
        let old = bridge.integrations.clone();
        old.lock().unwrap().list = true;
        old.lock().unwrap().pending = Some(("old".into(), None));
        bridge.detach(false);
        old.lock().unwrap().apply(ClientEvent::Response {
            request_id: "old".into(),
            response: serde_json::json!({"result":{}}),
        });
        assert!(!Arc::ptr_eq(&old, &bridge.integrations));
        let inbox = bridge.integrations.lock().unwrap();
        assert!(!inbox.list && !inbox.install && inbox.pending.is_none());
    }

    #[test]
    fn bells_move_once_titles_persist_and_old_inboxes_are_fenced() {
        use herdr_client::protocol::ServerMessage;
        let mut bridge = bridge();
        let old = bridge.inbox.clone();
        {
            let mut state = old.lock().unwrap();
            state.status = ConnectionStatus::Connected;
            state.apply(ClientEvent::Message(ServerMessage::TerminalBell {
                count: 2,
            }));
            state.apply(ClientEvent::Message(ServerMessage::WindowTitle {
                title: Some("agent".into()),
            }));
        }
        let update = bridge.take_update().unwrap();
        assert_eq!(update.bells, 2);
        assert_eq!(update.window_title.as_deref(), Some("agent"));
        old.lock().unwrap().set_outer_focus(true);
        let next = bridge.take_update().unwrap();
        assert_eq!(next.bells, 0, "a bell is delivered once");
        assert_eq!(
            next.window_title.as_deref(),
            Some("agent"),
            "a title is state"
        );

        bridge.reset(ConnectionStatus::Connected, false);
        {
            let mut state = old.lock().unwrap();
            state.apply(ClientEvent::Message(ServerMessage::TerminalBell {
                count: 1,
            }));
            state.apply(ClientEvent::Message(ServerMessage::WindowTitle {
                title: Some("late".into()),
            }));
        }
        let replacement = bridge.take_update().unwrap();
        assert_eq!(replacement.bells, 0);
        assert_eq!(replacement.window_title, None);
    }

    #[test]
    fn notifications_move_once_are_bounded_and_fenced_by_replacement() {
        use crate::notifications::{PENDING_LIMIT, tests::notification};
        use herdr_client::protocol::ServerMessage;
        let mut bridge = bridge();
        let old = bridge.inbox.clone();
        {
            let mut state = old.lock().unwrap();
            state.status = ConnectionStatus::Connected;
            for id in 0..100 {
                state.apply(ClientEvent::Message(ServerMessage::SemanticNotification(
                    notification(&id.to_string()),
                )));
            }
            state.set_outer_focus(true);
        }
        let update = bridge.take_update().unwrap();
        assert_eq!(update.notifications.len(), PENDING_LIMIT);
        assert_eq!(update.notifications[0].title, "92");
        assert_eq!(update.notifications[7].title, "99");
        assert_eq!(update.sound_events.len(), crate::sound::MAX_PENDING);
        assert_eq!(update.sound_events[0].1.title, "68");
        assert_eq!(update.sound_events[31].1.title, "99");
        assert!(bridge.take_update().is_none());
        old.lock().unwrap().set_outer_focus(false);
        let next = bridge.take_update().unwrap();
        assert!(next.notifications.is_empty());
        assert!(next.sound_events.is_empty());
        bridge.detach(false);
        assert!(update.sound_cancel.load(Ordering::Acquire));
        old.lock()
            .unwrap()
            .apply(ClientEvent::Message(ServerMessage::SemanticNotification(
                notification("late"),
            )));
        let detached = bridge.take_update().unwrap();
        assert!(detached.notifications.is_empty());
        assert!(detached.sound_events.is_empty());
        bridge.reset(ConnectionStatus::Connected, false);
        old.lock()
            .unwrap()
            .apply(ClientEvent::Message(ServerMessage::SemanticNotification(
                notification("late again"),
            )));
        let replacement = bridge.take_update().unwrap();
        assert!(replacement.notifications.is_empty());
        assert!(replacement.sound_events.is_empty());
        {
            let mut state = bridge.inbox.lock().unwrap();
            state.apply(ClientEvent::Message(ServerMessage::SemanticNotification(
                notification("discard on disconnect"),
            )));
            state.apply(ClientEvent::Disconnected {
                reason: "test".into(),
            });
        }
        let disconnected = bridge.take_update().unwrap();
        assert!(disconnected.notifications.is_empty());
        assert!(disconnected.sound_events.is_empty());
        assert!(disconnected.sound_cancel.load(Ordering::Acquire));
    }

    #[test]
    fn clipboard_writes_move_out_of_the_mailbox_exactly_once() {
        use herdr_client::protocol::ServerMessage;
        let bridge = bridge();
        bridge
            .inbox
            .lock()
            .unwrap()
            .apply(ClientEvent::Message(ServerMessage::Clipboard {
                data: "aGVsbG8=".into(),
            }));
        let update = bridge.take_update().unwrap();
        assert_eq!(
            update
                .clipboard_writes
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["hello"]
        );
        // Already delivered, so a poll without new work yields nothing.
        assert!(bridge.take_update().is_none());
        bridge.inbox.lock().unwrap().dirty = true;
        assert!(bridge.take_update().unwrap().clipboard_writes.is_empty());
    }

    #[test]
    fn contended_retirement_cancels_audio_after_boot_token_replacement() {
        for detach in [false, true] {
            let mut bridge = bridge();
            let inbox = bridge.inbox.clone();
            let mut held = inbox.lock().unwrap();
            held.sound_cancel = Arc::new(AtomicBool::new(false));
            if detach {
                bridge.detach(false);
                assert!(!bridge.sound_cancel.load(Ordering::Acquire));
            } else {
                drop(bridge);
            }
            assert!(held.sound_connection_cancel.load(Ordering::Acquire));
        }
    }

    #[test]
    fn synchronous_startup_failure_survives_mailbox_and_focus_updates() {
        let mut bridge = bridge();
        let mut options = ConnectOptions::default();
        options.surface_size.cols = 0;
        bridge.reconnect(options, true, true);
        let failed = bridge.take_update().unwrap();
        assert_eq!(failed.status, ConnectionStatus::Disconnected);
        assert!(failed.error.is_some());
        assert!(bridge.handle.is_none());
        assert!(bridge.take_update().is_none());
        bridge.inbox.lock().unwrap().set_outer_focus(false);
        let next = bridge.take_update().unwrap();
        assert_eq!(next.status, failed.status);
        assert_eq!(next.error, failed.error);
    }

    #[test]
    fn event_reader_startup_failure_is_authoritative() {
        let mut bridge = bridge();
        bridge.start(ConnectOptions::default(), true, |_| {
            Err(std::io::Error::other("reader startup failed"))
        });
        let state = bridge.take_update().unwrap();
        assert_eq!(state.status, ConnectionStatus::Disconnected);
        assert_eq!(state.error.as_deref(), Some("reader startup failed"));
        assert!(bridge.handle.is_none());
        bridge.inbox.lock().unwrap().set_outer_focus(true);
        assert_eq!(
            bridge.take_update().unwrap().status,
            ConnectionStatus::Disconnected
        );
    }

    #[test]
    fn detach_and_reconnect_fence_old_inboxes() {
        let mut bridge = bridge();
        let old = bridge.inbox.clone();
        old.lock().unwrap().local_daemon_peer = true;
        old.lock().unwrap().dialog_response = Some(("remove".into(), None));
        bridge.detach(true);
        old.lock().unwrap().apply(ClientEvent::Response {
            request_id: "remove".into(),
            response: serde_json::json!({"error":{"code":"dirty_worktree_requires_force"}}),
        });
        old.lock().unwrap().missing_installation = true;
        old.lock().unwrap().apply(ClientEvent::Disconnected {
            reason: "old connection".into(),
        });
        let detached = bridge.take_update().unwrap();
        assert_eq!(detached.status, ConnectionStatus::Detached);
        assert!(!detached.missing_installation);
        assert!(!detached.local_daemon_peer);
        assert!(detached.error.is_none());
        assert!(detached.dialog_response.is_none());
        assert!(detached.snapshot.is_none() && detached.surface.is_none());
        let old = bridge.inbox.clone();
        old.lock().unwrap().local_daemon_peer = true;
        let mut options = ConnectOptions::default();
        options.surface_size.cols = 0;
        bridge.reconnect(options, false, true);
        old.lock().unwrap().apply(ClientEvent::Disconnected {
            reason: "detached connection".into(),
        });
        let failed = bridge.take_update().unwrap();
        assert_eq!(failed.status, ConnectionStatus::Disconnected);
        assert!(!failed.local_daemon_peer);
        assert_ne!(failed.error.as_deref(), Some("detached connection"));
        assert!(!Arc::ptr_eq(&old, &bridge.inbox));
    }

    #[test]
    fn dialog_result_moves_out_once_without_losing_pending_registration() {
        let bridge = bridge();
        bridge.inbox.lock().unwrap().dialog_response = Some(("list".into(), None));
        assert!(matches!(
            bridge.take_update().unwrap().dialog_response,
            Some((id, None)) if id == "list"
        ));
        bridge.inbox.lock().unwrap().apply(ClientEvent::Response {
            request_id: "list".into(),
            response: serde_json::json!({"result":{}}),
        });
        assert!(
            bridge
                .take_update()
                .unwrap()
                .dialog_response
                .unwrap()
                .1
                .is_some()
        );
        bridge.inbox.lock().unwrap().set_outer_focus(true);
        assert!(matches!(
            bridge.take_update().unwrap().dialog_response,
            Some((id, None)) if id == "list"
        ));
    }
}
