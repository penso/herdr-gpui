//! The sessions a device unfolds in the device picker: the named sessions on
//! this machine, or the ones a saved device reports, each under the device it
//! runs on.
//! Both lists are probes — a session socket outlives its daemon, and a device's
//! sessions cost an SSH round trip — so only an unfolded device asks.
// i18n: migrated
use super::devices::picker::Line;
use super::{Page, colors, devices};
use crate::i18n::fl;
use crate::{HerdrWindow, endpoint::Endpoint, sessions::DeviceScan};
use gpui::{prelude::*, *};
use herdr_client::{ConnectTarget, LocalSession};
mod management;

pub(super) use management::{Edit, Target};

/// A line under an unfolded device, in the order it is painted. Only a row can
/// be selected, so navigation walks the `Row` entries and skips notes.
pub(super) enum Entry {
    /// A muted note, carrying the selector diagnostics and tests look for.
    Muted {
        selector: String,
        text: String,
    },
    Error(String),
    /// A selectable line, carrying what it paints: a row resolved again at paint
    /// time could describe a session its own scan has already replaced.
    Row {
        row: Row,
        spec: Spec,
    },
}

/// What a row attaches to. A device keeps its endpoint id rather than an index,
/// so a catalog change between paint and click cannot pick the wrong one, and it
/// names the session itself, because a device has many.
#[derive(Clone, PartialEq)]
pub(super) enum Row {
    Local(String),
    Device { id: String, session: String },
    Add(Target),
}

/// What one row paints. Both kinds share one shape so they cannot drift apart.
pub(super) struct Spec {
    label: SharedString,
    detail: SharedString,
    /// Running locally, or connected remotely.
    online: bool,
    checked: bool,
    enabled: bool,
}

impl Entry {
    pub(super) fn row(&self) -> Option<&Row> {
        match self {
            Self::Row { row, .. } => Some(row),
            _ => None,
        }
    }
}

/// The session a saved device attaches to. Asking the host what else it runs
/// would be an SSH round trip this popup does not take.
pub(super) fn target_session(target: &ConnectTarget) -> String {
    match target {
        ConnectTarget::Ssh { session, .. }
        | ConnectTarget::Wsl { session, .. }
        | ConnectTarget::Session { name: session, .. } => session.clone(),
        #[cfg(feature = "cloud")]
        ConnectTarget::Cloud { session, .. } => session.clone(),
        ConnectTarget::Socket(path) => path.display().to_string(),
        ConnectTarget::Local => "default".to_owned(),
    }
}

/// The state word a session paints, whether it was probed on this machine or
/// reported by its host.
fn state_word(running: bool) -> String {
    if running {
        fl!("sessions-menu-running")
    } else {
        fl!("sessions-menu-stopped")
    }
}

impl HerdrWindow {
    pub(super) fn finish_session_departure(&mut self) {
        // Selection is an ordinal shared with keyboard navigation. Once a row
        // disappears it must not transfer to the action now occupying its slot.
        if self.sessions.departure.is_some() && self.menu.page == Some(Page::Devices) {
            self.menu.selected = None;
        }
        self.sessions.finish_departure();
    }

    /// Opens the device picker with the device this window is on unfolded to
    /// its sessions, anchored where the footer's picker sits.
    pub(crate) fn open_sessions(
        &mut self,
        anchor: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.open_menu(window, cx) {
            return;
        }
        // Anchor to the control, not the pointer: every click position leaves the
        // same clear gap above the button.
        self.menu.anchor = anchor;
        self.menu.page = Some(Page::Devices);
        self.unfold_device(self.selected_endpoint);
        // The keys start on the session this window is on, among the others
        // they are opened to choose from.
        let current = self.picker_ordinal(
            |line| matches!(line, Line::Session(Entry::Row { spec, .. }) if spec.checked),
        );
        if current.is_some() {
            self.menu.selected = current;
        }
    }

    /// The socket this window's local endpoint would dial. `Local` resolves
    /// through the environment, so no session name can say which one is current.
    fn local_socket(&self) -> Option<std::path::PathBuf> {
        self.endpoints[0].connection.target.socket_path().ok()
    }

    /// The lines device `index` unfolds: its sessions, the notes that explain
    /// them, and its Add Session row. Index 0 is this machine.
    pub(super) fn device_session_entries(&self, index: usize) -> Vec<Entry> {
        let mut entries = Vec::new();
        if let Some(error) = &self.sessions.mutation_error {
            entries.push(Entry::Error(error.clone()));
        }
        if index == 0 {
            self.local_entries(&mut entries);
            // This machine's scan error belongs to its own sessions, not to a
            // saved device's.
            if let Some(error) = &self.sessions.error {
                entries.push(Entry::Error(error.clone()));
            }
        } else if let Some(endpoint) = self.endpoints.get(index) {
            self.device_entries(&mut entries, endpoint, index);
        }
        entries
    }

    /// One saved device's sessions, under the device's own row. The session it
    /// was saved with leads, and stands in for the list while that is still
    /// arriving or could not be read at all, so a host is never unreachable from
    /// the picker just because its session list is.
    fn device_entries(&self, entries: &mut Vec<Entry>, endpoint: &Endpoint, index: usize) {
        let saved = target_session(&endpoint.connection.target);
        let attached = self.selected_endpoint == index;
        let fallback = || {
            (
                saved.clone(),
                endpoint.live.status.to_string(),
                endpoint.live.status.is_connected(),
                attached,
            )
        };
        let (rows, note) = match self.sessions.devices.answers.get(&endpoint.id) {
            Some(DeviceScan::Sessions(sessions)) if !sessions.is_empty() => {
                let mut rows = Vec::with_capacity(sessions.len() + 1);
                // The session this device attaches to stays selectable even when
                // the host no longer lists it: the picker offers it too, because
                // the daemon starts a named session on demand.
                if !sessions.iter().any(|session| session.name == saved) {
                    rows.push(fallback());
                }
                rows.extend(
                    sessions
                        .iter()
                        .filter(|session| session.name == saved)
                        .chain(sessions.iter().filter(|session| session.name != saved))
                        .map(|session| {
                            (
                                session.name.clone(),
                                state_word(session.running),
                                session.running,
                                attached && session.name == saved,
                            )
                        }),
                );
                (rows, None)
            }
            Some(DeviceScan::Sessions(_)) => (
                vec![fallback()],
                Some(fl!("sessions-menu-no-device-sessions")),
            ),
            Some(DeviceScan::Failed(error)) => (vec![fallback()], Some(error.clone())),
            // A disabled device is never dialled, so it must not claim to be
            // waiting for an answer that is not coming.
            None if !endpoint.enabled => (vec![fallback()], Some(fl!("sessions-menu-disabled"))),
            None => (vec![fallback()], Some(fl!("sessions-menu-checking"))),
        };
        for (name, detail, online, checked) in rows {
            entries.push(Entry::Row {
                row: Row::Device {
                    id: endpoint.id.clone(),
                    session: name.clone(),
                },
                spec: Spec {
                    label: name.into(),
                    detail: detail.into(),
                    online,
                    checked,
                    enabled: endpoint.enabled,
                },
            });
        }
        if let Some(note) = note {
            entries.push(Entry::Muted {
                selector: format!("sessions-device-note-{}", endpoint.id),
                text: note,
            });
        }
        if let Some(host) = endpoint.connection.target.remote_host() {
            entries.push(self.add_session_entry(Target::Device {
                id: endpoint.id.clone(),
                host,
            }));
        }
    }

    /// This machine's sessions, with the notes that explain an empty list and what
    /// selecting a stopped session does.
    fn local_entries(&self, entries: &mut Vec<Entry>) {
        if self.sessions.entries.is_empty() {
            entries.push(Entry::Muted {
                selector: "sessions-empty".to_owned(),
                text: if self.sessions.scanning {
                    fl!("sessions-menu-checking")
                } else {
                    fl!("sessions-menu-no-local-sessions")
                },
            });
        }
        // A stopped session is still worth selecting: it starts its daemon, the
        // same way `herdr --session <name>` does.
        if self
            .sessions
            .entries
            .iter()
            .any(|session| !session.state.running())
        {
            entries.push(Entry::Muted {
                selector: "sessions-stopped".to_owned(),
                text: fl!("sessions-menu-stopped-note"),
            });
        }
        entries.extend(self.sessions.entries.iter().map(|session| Entry::Row {
            row: Row::Local(session.name.clone()),
            spec: self.local_session_spec(session),
        }));
        entries.push(self.add_session_entry(Target::Local));
    }

    fn local_session_spec(&self, session: &LocalSession) -> Spec {
        Spec {
            label: session.name.clone().into(),
            detail: state_word(session.state.running()).into(),
            online: session.state.running(),
            checked: self
                .local_socket()
                .is_some_and(|socket| socket == session.socket),
            enabled: true,
        }
    }

    /// One session row under its device. `row` counts the session rows painted
    /// so far and names the row's selectors; `ordinal` is its place among the
    /// picker's selectable lines, which hover and the keys address.
    pub(super) fn sessions_row(
        &self,
        row: usize,
        ordinal: usize,
        choice: Row,
        selected: bool,
        spec: &Spec,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = &self.theme;
        let font = &self.config.ui;
        let matches_target = |target: &ConnectTarget| match (&choice, target) {
            (Row::Local(name), ConnectTarget::Session { name: deleted, .. }) => name == deleted,
            (Row::Device { id, session }, target) if target.is_remote() => {
                target.remote_session() == Some(session.as_str())
                    && self.endpoints.iter().any(|endpoint| {
                        endpoint.id == *id
                            && endpoint.connection.target.remote_host() == target.remote_host()
                    })
            }
            _ => false,
        };
        let deleting = self
            .sessions
            .mutation_target
            .as_ref()
            .is_some_and(&matches_target);
        let remaining = self
            .sessions
            .departure
            .as_ref()
            .filter(|departure| matches_target(&departure.target))
            .map(|departure| departure.remaining(crate::motion::now()));
        // Two lines plus padding; a wrapped note makes the row taller.
        let height = font.line_height() * 2. + 16.;
        let removal = choice.clone();
        let removal_target = self.management_target(&choice);
        let removable = !matches!(choice, Row::Add(_));
        let deletion_color = if self.session_delete_reason(&choice).is_none() {
            colors::danger(theme)
        } else if selected || spec.checked {
            rgb(theme.foreground)
        } else {
            rgb(theme.muted)
        };
        let view = div()
            .id(("session-entry", row))
            .debug_selector(move || format!("sessions-row-{row}"))
            .p(px(8.))
            .min_h(px(height))
            .line_height(px(font.line_height()))
            .rounded(px(crate::config::corners::CONTROL))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.))
            .when(selected, |s| s.bg(rgb(theme.active)))
            .when(spec.checked, |s| s.bg(rgb(theme.primary_wash())))
            .when(spec.enabled, |s| s.cursor_pointer())
            .text_color(rgb(if spec.enabled {
                theme.foreground
            } else {
                theme.muted
            }))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .when(spec.checked, |label| {
                                label
                                    .debug_selector(move || format!("sessions-current-{row}"))
                                    .font_weight(FontWeight::SEMIBOLD)
                            })
                            .child(spec.label.clone()),
                    )
                    .child(
                        // Wraps rather than cutting a note short: the popup's
                        // width is the device list's, not the note's.
                        div()
                            .debug_selector(move || format!("sessions-detail-{row}"))
                            .min_w_0()
                            .text_size(px(font.size * 0.85))
                            .text_color(rgb(theme.muted))
                            .child(if deleting {
                                fl!("sessions-menu-deleting").into()
                            } else {
                                spec.detail.clone()
                            }),
                    ),
            )
            .when(removable, |container| {
                container.child(
                    div()
                        .debug_selector(move || format!("sessions-dot-{row}"))
                        .size(px(7.))
                        .flex_none()
                        .rounded_full()
                        .bg(rgb(if spec.online {
                            colors::online(theme)
                        } else {
                            theme.muted
                        })),
                )
            })
            .when(removable, |container| {
                container.child(
                    super::action_icon(
                        "icons/trash.svg",
                        format!("sessions-delete-icon-{row}"),
                        deletion_color,
                        rgb(theme.foreground),
                    )
                    .id(("session-delete", row))
                    .debug_selector(move || format!("sessions-delete-{row}"))
                    .hover(|s| s.bg(colors::action_hover(theme)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.confirm_session_delete(
                            removal.clone(),
                            removal_target.clone(),
                            window,
                            cx,
                        );
                    })),
                )
            })
            // Sits in the trash column; the whole row is the click target.
            .when(!removable, |container| {
                let color = rgb(if spec.enabled {
                    theme.foreground
                } else {
                    theme.muted
                });
                container.child(super::action_icon(
                    "icons/plus.svg",
                    format!("sessions-add-icon-{row}"),
                    color,
                    color,
                ))
            })
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                if *hovered {
                    this.menu.selected = Some(ordinal);
                    cx.notify();
                }
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                // Keep the painted identity even if the catalog refreshes.
                this.choose_session(choice.clone(), window, cx);
            }));
        if let Some(remaining) = remaining {
            div()
                .debug_selector(move || format!("sessions-departing-{row}"))
                // Keep the anchored popup's height and every row position stable
                // until the fade finishes; then remove the row in one update.
                // The row sizes the wrapper, so a wrapped note keeps its height.
                .flex_none()
                .opacity(remaining)
                .child(view)
                .into_any_element()
        } else {
            view.into_any_element()
        }
    }

    /// A note under an unfolded device: why its list is empty or partial, or
    /// what went wrong. `None` for a row, which [`Self::sessions_row`] paints.
    pub(super) fn session_note(&self, entry: &Entry) -> Option<AnyElement> {
        let theme = &self.theme;
        let font = &self.config.ui;
        let note = div().px(px(8.)).py(px(2.)).text_size(px(font.size * 0.85));
        match entry {
            Entry::Muted { selector, text } => {
                let selector = selector.clone();
                Some(
                    note.debug_selector(move || selector.clone())
                        .text_color(rgb(theme.muted))
                        .child(text.clone())
                        .into_any_element(),
                )
            }
            Entry::Error(text) => Some(
                note.debug_selector(|| "sessions-error".into())
                    .text_color(colors::danger(theme))
                    .child(text.clone())
                    .into_any_element(),
            ),
            Entry::Row { .. } => None,
        }
    }

    /// Attach to a row's session. The row is what the caller painted, not an
    /// index into a list computed again here: a device list that refreshed in
    /// between would otherwise have the click name a different session.
    pub(super) fn choose_session(&mut self, row: Row, window: &mut Window, cx: &mut Context<Self>) {
        if self.sessions.mutation.is_some() {
            return;
        }
        match row {
            Row::Add(target) => self.open_session_create(target, window, cx),
            Row::Local(name) => {
                self.dismiss_menu(window, cx);
                self.select_local_session(&name, cx);
                // The picker's own follow-up: the sidebar follows the host that
                // was just chosen, and another session's tree is not this one's.
                self.set_device_scope(devices::DeviceScope::only(self.endpoints[0].id.clone()));
            }
            // A device row is one of that device's sessions, so it retargets the
            // device rather than adding an endpoint for every session it runs.
            Row::Device { id, session } => {
                // A disabled device has nothing to attach to, and the picker
                // refuses the same click. Filtering the sidebar to it would leave
                // the host list empty, so the menu stays as it is.
                if !self
                    .endpoints
                    .iter()
                    .any(|endpoint| endpoint.id == id && endpoint.enabled)
                {
                    return;
                }
                self.dismiss_menu(window, cx);
                self.select_device_session(&id, &session, cx);
                // The local branch's own follow-up: the sidebar follows the host
                // that was just chosen, and another session's tree is not this
                // one's.
                self.set_device_scope(devices::DeviceScope::only(id));
            }
        }
    }

    /// Keys for the open Add Session or Delete Session form.
    pub(super) fn sessions_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(&self.menu.session_edit, Some(Edit::Create { input, .. })
            if input.read(cx).is_composing() || !matches!(event.keystroke.key.as_str(), "enter" | "escape"))
        {
            return;
        }
        cx.stop_propagation();
        window.prevent_default();
        self.session_edit_key(event, window, cx);
    }
}
