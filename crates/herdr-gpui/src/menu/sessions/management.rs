//! Picker forms capture device identity and host, never a mutable row ordinal.
use super::*;
use crate::search_input::SearchInput;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::menu) enum Target {
    Local,
    Device { id: String, host: String },
}

pub(in crate::menu) enum Edit {
    Create {
        target: Target,
        input: Entity<SearchInput>,
    },
    Delete {
        row: Row,
        target: Target,
    },
}

impl HerdrWindow {
    pub(super) fn local_management_available(&self) -> bool {
        !self.local_development()
            && !matches!(
                self.endpoints[0].connection.target,
                ConnectTarget::Socket(_)
            )
    }

    pub(super) fn add_session_entry(target: Target, enabled: bool) -> Entry {
        let detail = if enabled {
            "Create and connect"
        } else {
            match &target {
                Target::Local => "Unavailable with --socket or --dev",
                Target::Device { .. } if cfg!(windows) => "SSH is unavailable on Windows",
                Target::Device { .. } => "Enable this device to manage sessions",
            }
        };
        Entry::Row {
            row: Row::Add(target),
            spec: Spec {
                label: "Add session…".into(),
                detail: detail.into(),
                online: false,
                checked: false,
                enabled,
            },
        }
    }

    pub(super) fn management_target(&self, row: &Row) -> Option<Target> {
        match row {
            Row::Local(_) if self.local_management_available() => Some(Target::Local),
            Row::Device { id, .. } => self
                .endpoints
                .iter()
                .find(|e| e.id == *id && e.enabled)
                .and_then(|e| match &e.connection.target {
                    ConnectTarget::Ssh { target, .. } if !cfg!(windows) => Some(Target::Device {
                        id: id.clone(),
                        host: target.clone(),
                    }),
                    _ => None,
                }),
            _ => None,
        }
    }

    fn target_available(&self, target: &Target) -> bool {
        match target {
            Target::Local => self.local_management_available(),
            Target::Device { id, host } => self.endpoints.iter().any(|e| e.id == *id && e.enabled
                && matches!(&e.connection.target, ConnectTarget::Ssh { target, .. } if target == host)) && !cfg!(windows),
        }
    }

    pub(super) fn session_delete_reason(&self, row: &Row) -> Option<&'static str> {
        if self.sessions.mutation.is_some() {
            return Some("Wait for session deletion to finish.");
        }
        let name = match row {
            Row::Local(name) | Row::Device { session: name, .. } => name,
            Row::Add(_) => return Some("Select a session to delete."),
        };
        if name == "default" {
            return Some("Herdr cannot delete the default session.");
        }
        let Some(target) = self.management_target(row) else {
            return Some(
                "Session management is unavailable for explicit sockets, development catalogs, or disabled devices.",
            );
        };
        match target {
            Target::Local => {
                if !self.sessions.entries.iter().any(|s| s.name == *name) {
                    return Some("Refresh the session list before deleting.");
                }
            }
            Target::Device { id, host } => {
                if self.endpoints.iter().any(|e| {
                    e.saved_ssh()
                        .is_some_and(|(target, session)| target == host && session == name)
                }) {
                    return Some(
                        "A saved device profile uses this session. Remove or reconfigure that profile before deleting it.",
                    );
                }
                let Some(DeviceScan::Sessions(sessions)) = self.sessions.devices.answers.get(&id)
                else {
                    return Some("Wait for this device's session list before deleting.");
                };
                if !sessions.iter().any(|s| s.name == *name) {
                    return Some("This session is not in the device's latest list.");
                }
            }
        }
        None
    }

    pub(super) fn open_session_create(
        &mut self,
        target: Target,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.target_available(&target) || self.sessions.mutation.is_some() {
            return;
        }
        let input = cx.new(|cx| {
            let mut input = SearchInput::new(cx);
            input.set_placeholder("Session name", cx);
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
            input
        });
        window.focus(&input.read(cx).focus.clone(), cx);
        self.menu.error = None;
        self.menu.session_edit = Some(Edit::Create { target, input });
        cx.notify();
    }

    pub(super) fn confirm_session_delete(
        &mut self,
        row: Row,
        painted_target: Option<Target>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.management_target(&row) != painted_target {
            self.sessions.mutation_error =
                Some("The device changed. Reopen the session picker.".into());
            cx.notify();
            return;
        }
        if let Some(reason) = self.session_delete_reason(&row) {
            self.sessions.mutation_error = Some(reason.into());
        } else if let Some(target) = self.management_target(&row) {
            self.menu.error = None;
            self.menu.session_edit = Some(Edit::Delete { row, target });
            window.focus(&self.menu.focus, cx);
        }
        cx.notify();
    }

    fn cancel_session_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu.session_edit = None;
        self.menu.error = None;
        window.focus(&self.menu.focus, cx);
        cx.notify();
    }

    fn submit_session_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sessions.mutation.is_some() {
            return;
        }
        let Some(edit) = &self.menu.session_edit else {
            return;
        };
        match edit {
            Edit::Create { target, input } => {
                if input.read(cx).is_composing() {
                    return;
                }
                let name = input.read(cx).text().to_owned();
                let target = target.clone();
                if !self.target_available(&target) {
                    self.menu.error = Some(
                        "The device changed or is unavailable. Reopen the session picker.".into(),
                    );
                    cx.notify();
                    return;
                }
                if herdr_client::session_socket(std::path::Path::new(""), &name).is_err() {
                    self.menu.error = Some("Use 1–64 ASCII letters, digits, dots, underscores or hyphens. The names ‘.’ and ‘..’ are not allowed.".into());
                    cx.notify();
                    return;
                }
                let duplicate = name == "default"
                    || match &target {
                        Target::Local => self
                            .sessions
                            .entries
                            .iter()
                            .any(|s| s.name.eq_ignore_ascii_case(&name)),
                        Target::Device { id, .. } => {
                            matches!(self.sessions.devices.answers.get(id),
                        Some(DeviceScan::Sessions(sessions)) if sessions.iter().any(|s| s.name == name))
                        }
                    };
                if duplicate {
                    self.menu.error =
                        Some("That session already exists. Select it from the list.".into());
                    cx.notify();
                    return;
                }
                let row = match target {
                    Target::Local => Row::Local(name),
                    Target::Device { id, .. } => Row::Device { id, session: name },
                };
                // The normal connection worker starts `herdr --session NAME
                // server` locally or remote-client-bridge over SSH. Startup
                // errors belong to that connection's authoritative inbox.
                self.sessions.refresh();
                self.sessions.devices.refresh();
                self.choose_session(row, window, cx);
            }
            Edit::Delete { row, target } => {
                if self.management_target(row).as_ref() != Some(target) {
                    self.menu.error = Some("The device changed. Reopen the session picker.".into());
                    cx.notify();
                    return;
                }
                if let Some(reason) = self.session_delete_reason(row) {
                    self.menu.error = Some(reason.into());
                    cx.notify();
                    return;
                }
                let name = match row {
                    Row::Local(name) | Row::Device { session: name, .. } => name.clone(),
                    Row::Add(_) => return,
                };
                let target = target.clone();
                let connection = match &target {
                    Target::Local => ConnectTarget::Session {
                        name: name.clone(),
                        development: false,
                    },
                    Target::Device { host, .. } => ConnectTarget::Ssh {
                        target: host.clone(),
                        session: name.clone(),
                    },
                };
                // Retire old inboxes before stopping the daemon, so this window
                // cannot reconnect to and recreate the name being deleted.
                if self.retarget_session_for_deletion(&connection) {
                    self.reconnect();
                    self.menu.page = Some(Page::Sessions);
                    self.menu.selected = Some(0);
                }
                let background = cx.background_executor().spawn(async move {
                    match target {
                        Target::Local => {
                            herdr_client::delete_local_session(&crate::daemon::executable(), &name)
                        }
                        Target::Device { host, .. } => {
                            herdr_client::delete_remote_session(&host, &name)
                        }
                    }
                });
                self.sessions.mutation_error = None;
                self.sessions.mutation_target = Some(connection.clone());
                self.cancel_session_edit(window, cx);
                let timer = cx.background_executor().clone();
                self.sessions.mutation = Some(cx.spawn(async move |this, cx| {
                    let result = background.await;
                    if result.is_ok() {
                        let _ = this.update(cx, |this, cx| {
                            this.sessions.departure = Some(crate::sessions::Departure::new(
                                connection,
                                std::time::Instant::now(),
                            ));
                            cx.notify();
                        });
                        timer.timer(crate::sessions::DELETION_ANIMATION).await;
                    }
                    let _ = this.update(cx, |this, cx| {
                        this.finish_session_departure();
                        this.sessions.mutation = None;
                        this.sessions.mutation_target = None;
                        this.sessions.mutation_error = result.err().map(|error| error.to_string());
                        // Refresh even after a timeout: the CLI may have applied
                        // the operation before its answer was lost.
                        this.sessions.refresh();
                        this.sessions.devices.refresh();
                        cx.notify();
                    });
                }));
            }
        }
    }

    pub(super) fn session_edit_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(&self.menu.session_edit, Some(Edit::Create { input, .. }) if input.read(cx).is_composing())
        {
            return;
        }
        match event.keystroke.key.as_str() {
            "enter" => self.submit_session_edit(window, cx),
            "escape" => self.cancel_session_edit(window, cx),
            _ => {}
        }
    }

    pub(super) fn render_session_edit(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let mut view = div()
            .debug_selector(|| "session-modal".into())
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(12.));
        match &self.menu.session_edit {
            Some(Edit::Create { input, target }) => {
                let destination = match target {
                    Target::Local => "this machine",
                    Target::Device { host, .. } => host,
                };
                view = view.child(div().text_size(px(self.config.ui.size * 1.35))
                    .font_weight(FontWeight::SEMIBOLD).child("Add session"))
                    .child(div().text_color(rgb(theme.muted)).child(format!("On {destination}")))
                    .child(input.clone())
                    .child(div().text_color(rgb(theme.muted)).child("Creates and connects to a headless session. An existing name connects to that session."));
            }
            Some(Edit::Delete { row, .. }) => {
                let name = match row {
                    Row::Local(name) | Row::Device { session: name, .. } => name.as_str(),
                    Row::Add(_) => "",
                };
                view = view
                    .child(
                        div()
                            .text_size(px(self.config.ui.size * 1.35))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Delete session?"),
                    )
                    .child(div().min_w_0().truncate().child(name.to_owned()))
                    .child(div().text_color(rgb(theme.muted)).child(
                        "Herdr will stop this session, terminate its running processes, and remove its saved state. If you are using it, this window switches to default. This cannot be undone.",
                    ));
            }
            None => return view,
        }
        if let Some(error) = &self.menu.error {
            view = view.child(
                div()
                    .text_color(colors::danger(&self.theme))
                    .child(error.clone()),
            );
        }
        view.child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.))
                .child(
                    div()
                        .id("session-edit-cancel")
                        .debug_selector(|| "session-edit-cancel".into())
                        .px(px(12.))
                        .py(px(6.))
                        .rounded(px(crate::config::corners::CONTROL))
                        .border_1()
                        .border_color(rgb(theme.active))
                        .hover(|s| s.bg(rgb(theme.active)))
                        .cursor_pointer()
                        .child("Cancel")
                        .on_click(
                            cx.listener(|this, _, window, cx| this.cancel_session_edit(window, cx)),
                        ),
                )
                .child(
                    div()
                        .id("session-edit-submit")
                        .debug_selector(|| "session-edit-submit".into())
                        .px(px(12.))
                        .py(px(6.))
                        .rounded(px(crate::config::corners::CONTROL))
                        .border_1()
                        .border_color(rgb(theme.active))
                        .bg(rgb(theme.active))
                        .when(
                            matches!(self.menu.session_edit, Some(Edit::Delete { .. })),
                            |s| s.text_color(colors::danger(theme)),
                        )
                        .cursor_pointer()
                        .child(
                            if matches!(self.menu.session_edit, Some(Edit::Create { .. })) {
                                "Create and connect"
                            } else {
                                "Delete permanently"
                            },
                        )
                        .on_click(
                            cx.listener(|this, _, window, cx| this.submit_session_edit(window, cx)),
                        ),
                ),
        )
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{Edit, Target};
    use crate::sidebar::layout_tests::{fixture_window, full_draw};
    use crate::{
        endpoint::Endpoint,
        menu::{Page, sessions::Row},
        sessions::DeviceScan,
    };
    use gpui::{TestAppContext, point, px, size};
    use herdr_client::{ConnectTarget, LocalSession, SessionState};

    #[gpui::test]
    fn create_form_owns_text_validates_and_returns_focus(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.simulate_resize(size(px(800.), px(600.)));
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                // No valid submission in this fixture: it must never start a daemon.
                view.endpoints[0].connection.target = ConnectTarget::Session {
                    name: "default".into(),
                    development: false,
                };
                view.open_sessions(point(px(10.), px(550.)), window, cx);
                view.open_session_create(Target::Local, window, cx);
            })
        });
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        let panel = cx.debug_bounds("menu-panel").unwrap();
        assert_eq!(panel.center(), point(px(400.), px(300.)));
        assert_eq!(panel.size.width, px(480.));
        assert!(cx.debug_bounds("session-modal").is_some());
        cx.simulate_keystrokes("b a d space n a m e enter");
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                let Some(Edit::Create { input, .. }) = &view.menu.session_edit else {
                    panic!("form disappeared")
                };
                assert_eq!(input.read(cx).text(), "bad name");
                assert!(view.menu.error.is_some());
                assert!(view.sessions.mutation.is_none());
                input.update(cx, |input, cx| input.set_text_selected("default", cx));
                view.submit_session_edit(window, cx);
                assert!(view.menu.error.as_ref().unwrap().contains("already exists"));
            })
        });
        cx.simulate_keystrokes("escape");
        cx.update(|window, cx| {
            let view = view.read(cx);
            assert!(view.menu.session_edit.is_none());
            assert!(view.menu.focus.is_focused(window));
            assert_eq!(view.menu.page, Some(Page::Sessions));
        });
    }

    #[gpui::test]
    fn delete_confirmation_is_centered_and_cancel_returns_to_picker(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(fixture_window);
        let name = "a-long-stopped-session-name-that-must-stay-inside-the-modal";
        cx.simulate_resize(size(px(640.), px(480.)));
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.endpoints[0].connection.target = ConnectTarget::Session {
                    name: "default".into(),
                    development: false,
                };
                view.sessions.entries = vec![LocalSession {
                    name: name.into(),
                    state: SessionState::Stopped,
                    socket: ConnectTarget::Session {
                        name: name.into(),
                        development: false,
                    }
                    .socket_path()
                    .unwrap(),
                }];
                view.open_sessions(point(px(10.), px(440.)), window, cx);
            })
        });
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        let picker = cx.debug_bounds("menu-panel").unwrap();
        let row = cx.debug_bounds("sessions-row-0").unwrap();
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.confirm_session_delete(
                    Row::Local(name.into()),
                    Some(Target::Local),
                    window,
                    cx,
                );
            })
        });
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        let panel = cx.debug_bounds("menu-panel").unwrap();
        assert_eq!(cx.debug_bounds("session-picker-underlay").unwrap(), picker);
        assert_eq!(
            cx.debug_bounds("sessions-row-0").unwrap(),
            row,
            "the modal must cover, not replace, the picker"
        );
        assert_eq!(panel.center(), point(px(320.), px(240.)));
        let cancel = cx.debug_bounds("session-edit-cancel").unwrap();
        let submit = cx.debug_bounds("session-edit-submit").unwrap();
        assert!(cancel.left() >= panel.left() && submit.right() <= panel.right());
        cx.simulate_click(cancel.center(), gpui::Modifiers::default());
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
        assert_eq!(cx.debug_bounds("menu-panel").unwrap(), picker);
        cx.update(|_, cx| {
            let view = view.read(cx);
            assert!(view.menu.session_edit.is_none());
            assert_eq!(view.menu.page, Some(Page::Sessions));
            assert!(view.sessions.mutation.is_none());
        });
    }

    #[gpui::test]
    fn deletion_confirms_running_and_current_sessions_but_protects_default(
        cx: &mut TestAppContext,
    ) {
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.endpoints[0].connection.target = ConnectTarget::Session {
                    name: "current".into(),
                    development: false,
                };
                view.sessions.entries = [
                    ("default", SessionState::Stopped),
                    ("current", SessionState::Stopped),
                    ("busy", SessionState::Running),
                    ("old", SessionState::Stopped),
                ]
                .into_iter()
                .map(|(name, state)| LocalSession {
                    name: name.into(),
                    state,
                    socket: ConnectTarget::Session {
                        name: name.into(),
                        development: false,
                    }
                    .socket_path()
                    .unwrap(),
                })
                .collect();
                for name in ["default", "missing"] {
                    assert!(
                        view.session_delete_reason(&Row::Local(name.into()))
                            .is_some(),
                        "{name}"
                    );
                }
                for name in ["current", "busy"] {
                    assert!(
                        view.session_delete_reason(&Row::Local(name.into()))
                            .is_none()
                    );
                    view.confirm_session_delete(
                        Row::Local(name.into()),
                        Some(Target::Local),
                        window,
                        cx,
                    );
                    assert!(matches!(view.menu.session_edit, Some(Edit::Delete { .. })));
                    assert!(view.sessions.mutation.is_none());
                    view.cancel_session_edit(window, cx);
                }
                assert!(
                    view.session_delete_reason(&Row::Local("old".into()))
                        .is_none()
                );
                view.confirm_session_delete(
                    Row::Local("old".into()),
                    Some(Target::Local),
                    window,
                    cx,
                );
                assert!(matches!(view.menu.session_edit, Some(Edit::Delete { .. })));
                view.cancel_session_edit(window, cx);
                assert!(
                    view.sessions.mutation.is_none(),
                    "confirmation alone must not spawn"
                );
                view.endpoints[0].connection.target = ConnectTarget::Session {
                    name: "default".into(),
                    development: true,
                };
                assert!(
                    view.session_delete_reason(&Row::Local("old".into()))
                        .is_some()
                );
                view.open_session_create(Target::Local, window, cx);
                assert!(view.menu.session_edit.is_none());
            })
        });
    }

    #[gpui::test]
    fn a_replaced_device_cannot_receive_a_captured_deletion(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.endpoints.push(Endpoint::new(
                    "ssh:test".into(),
                    "Test".into(),
                    ConnectTarget::Ssh {
                        target: "old.invalid".into(),
                        session: "default".into(),
                    },
                    true,
                ));
                view.sessions.devices.answers.insert(
                    "ssh:test".into(),
                    DeviceScan::Sessions(vec![herdr_client::RemoteSession {
                        name: "old".into(),
                        running: false,
                    }]),
                );
                let row = Row::Device {
                    id: "ssh:test".into(),
                    session: "old".into(),
                };
                // Inject a captured confirmation on all platforms; Windows refuses
                // management_target even before considering the replacement.
                view.menu.session_edit = Some(Edit::Delete {
                    row,
                    target: Target::Device {
                        id: "ssh:test".into(),
                        host: "old.invalid".into(),
                    },
                });
                view.endpoints[1].connection.target = ConnectTarget::Ssh {
                    target: "replacement.invalid".into(),
                    session: "default".into(),
                };
                view.submit_session_edit(window, cx);
                assert!(view.menu.error.as_ref().unwrap().contains("changed"));
                assert!(view.sessions.mutation.is_none());
                let Some(Edit::Delete { row, target }) = view.menu.session_edit.take() else {
                    panic!("the rejected confirmation must remain open");
                };
                // The same fence starts at paint, not only at confirmation.
                view.confirm_session_delete(row, Some(target), window, cx);
                assert!(view.menu.session_edit.is_none());
                assert!(
                    view.sessions
                        .mutation_error
                        .as_ref()
                        .unwrap()
                        .contains("changed")
                );
            })
        });
    }

    #[gpui::test]
    fn confirmed_current_session_handoff_retires_only_matching_targets(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(fixture_window);
        cx.update(|_, cx| {
            view.update(cx, |view, _| {
                let named = ConnectTarget::Session {
                    name: "demo".into(),
                    development: false,
                };
                view.endpoints[0].connection.target = named.clone();
                let old = view.endpoints[0].connection.inbox.clone();
                assert!(view.retarget_session_for_deletion(&named));
                assert_eq!(
                    view.endpoints[0].connection.target,
                    ConnectTarget::Session {
                        name: "default".into(),
                        development: false
                    }
                );
                assert!(!std::sync::Arc::ptr_eq(
                    &old,
                    &view.endpoints[0].connection.inbox
                ));
                assert!(!view.retarget_session_for_deletion(&named));
                for (id, host, session) in [
                    ("ssh:one", "host.invalid", "demo"),
                    ("ssh:two", "host.invalid", "demo"),
                    ("ssh:other", "other.invalid", "demo"),
                ] {
                    view.endpoints.push(Endpoint::new(
                        id.into(),
                        id.into(),
                        ConnectTarget::Ssh {
                            target: host.into(),
                            session: session.into(),
                        },
                        true,
                    ));
                }
                view.selected_endpoint = 1;
                let remote = ConnectTarget::Ssh {
                    target: "host.invalid".into(),
                    session: "demo".into(),
                };
                assert!(view.retarget_session_for_deletion(&remote));
                assert_eq!(view.selected_endpoint, 1);
                for endpoint in &view.endpoints[1..3] {
                    assert_eq!(
                        endpoint.connection.target,
                        ConnectTarget::Ssh {
                            target: "host.invalid".into(),
                            session: "default".into()
                        }
                    );
                    assert!(endpoint.connection.handle.is_none());
                }
                assert_eq!(
                    view.endpoints[3].connection.target,
                    ConnectTarget::Ssh {
                        target: "other.invalid".into(),
                        session: "demo".into()
                    }
                );
            })
        });
    }
}
