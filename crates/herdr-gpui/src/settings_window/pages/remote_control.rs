//! Remote control: the daemon's switch and how it serves, what it reports
//! (addresses, certificate fingerprint, devices), pairing, and revoking a
//! device after asking here. Settings save through the window's serial
//! path; the daemon's status comes from [`Supervisor`], which polls it often
//! while this page holds a lease.

use super::{SettingsWindow, on_value, ui};
use crate::{
    config::{
        preferences::Preference,
        remote::{Bind, Certificate, Edit, Privacy},
    },
    remote::{Health, Supervisor, supervisor::Lease},
    search_input::SearchInput,
    settings_window::{
        Section,
        inputs::{DropdownField, NumberField},
    },
};
use gpui::{prelude::*, *};
use herdr_remote_control::control::{Device, DeviceId};
use std::path::PathBuf;

/// The page's state while it shows.
#[derive(Default)]
pub(in crate::settings_window) struct RemoteSettings {
    /// Keeps the daemon's status and devices fresh while the page shows.
    lease: Option<Lease>,
    /// The device whose Revoke waits for confirmation.
    pub(in crate::settings_window) revoking: Option<DeviceId>,
    /// "My files" was picked, before any files are saved.
    files: bool,
    cert: Option<Entity<SearchInput>>,
    key: Option<Entity<SearchInput>>,
    /// The files typed are not absolute paths, so they were not saved.
    invalid: bool,
}

/// Opens Settings at Remote control, as the status bar's phone does.
pub(crate) fn open(source: WeakEntity<crate::HerdrWindow>, cx: &mut App) {
    crate::settings_window::open(source, cx);
    // Queued after `open`'s own deferred work, so the window exists by now.
    cx.defer(|cx| {
        let Some(handle) = cx
            .default_global::<crate::settings_window::SettingsWindowHandle>()
            .window
        else {
            return;
        };
        let _ = handle.update(cx, |view, window, cx| {
            view.select_section(Section::RemoteControl, window, cx);
        });
    });
}

/// "All interfaces", then each address the daemon serves on, then the
/// configured one if the daemon does not list it.
pub(in crate::settings_window) fn bind_choices(bind: Bind, health: &Health) -> Vec<Bind> {
    let mut choices = vec![Bind::All];
    if let Health::Running(status) = health {
        choices.extend(status.addresses.iter().filter_map(|origin| {
            let host = origin.strip_prefix("https://").unwrap_or(origin);
            let host = host.rsplit_once(':').map_or(host, |(host, _)| host);
            host.trim_start_matches('[')
                .trim_end_matches(']')
                .parse()
                .ok()
                .map(Bind::Address)
        }));
    }
    if !choices.contains(&bind) {
        choices.push(bind);
    }
    choices
}

fn bind_label(bind: Bind) -> String {
    match bind {
        Bind::All => "All interfaces".to_owned(),
        Bind::Address(address) => address.to_string(),
    }
}

/// One line about the daemon, for the switch's row.
pub(in crate::settings_window) fn health_line(health: &Health, busy: bool) -> String {
    if busy {
        return "Updating the service\u{2026}".to_owned();
    }
    match health {
        Health::Off => "Off. Turning it on installs a user service that serves your chats over \
                        HTTPS on your network, and over Tailscale when it runs."
            .to_owned(),
        Health::Unknown => "Starting\u{2026}".to_owned(),
        Health::Running(status) => {
            let devices = match status.devices {
                1 => "1 device".to_owned(),
                count => format!("{count} devices"),
            };
            let mut line = format!(
                "Running on port {} \u{b7} {devices}, {} connected",
                status.port, status.connected
            );
            if !status.herdr {
                line.push_str(" \u{b7} Herdr is not running");
            }
            line
        }
        Health::NotRunning(reason) => format!(
            "Not running: {}",
            crate::notifications::safe_text(reason, 200)
        ),
    }
}

/// "Last seen 2 h ago · Notifications on · Connected".
pub(in crate::settings_window) fn device_line(device: &Device, now_ms: u64) -> String {
    let seen = match device.last_seen_ms {
        _ if device.connected => "Connected now".to_owned(),
        Some(seen) => format!("Last seen {}", crate::remote::ago(seen, now_ms)),
        None => "Never connected".to_owned(),
    };
    let push = if device.push {
        "notifications on"
    } else {
        "notifications off"
    };
    format!("{seen} \u{b7} {push}")
}

impl SettingsWindow {
    /// The page shows: keep the daemon's details fresh, and make the
    /// certificate fields.
    pub(in crate::settings_window) fn open_remote_control(&mut self, cx: &mut Context<Self>) {
        if self.remote.lease.is_none() {
            self.remote.lease = Supervisor::lease(cx);
        }
        let (cert, key) = match &self.config.remote.certificate {
            Certificate::Files { cert, key } => {
                (cert.display().to_string(), key.display().to_string())
            }
            Certificate::Own => Default::default(),
        };
        let field = |text: String, placeholder: &'static str, cx: &mut Context<Self>| {
            let input = cx.new(SearchInput::new);
            input.update(cx, |input, cx| {
                input.set_text_selected(&text, cx);
                input.set_placeholder(placeholder, cx);
            });
            input
        };
        if self.remote.cert.is_none() {
            self.remote.cert = Some(field(cert, "/path/to/certificate.pem", cx));
        }
        if self.remote.key.is_none() {
            self.remote.key = Some(field(key, "/path/to/private-key.pem", cx));
        }
        self.sync_remote_fields(cx);
    }

    /// The page went away.
    pub(in crate::settings_window) fn close_remote_control(&mut self) {
        self.remote.lease = None;
        self.remote.revoking = None;
    }

    pub(in crate::settings_window) fn sync_remote_fields(&mut self, cx: &mut Context<Self>) {
        for input in [&self.remote.cert, &self.remote.key].into_iter().flatten() {
            input.update(cx, |input, cx| {
                input.set_appearance(self.config.terminal.clone(), self.theme.clone(), cx);
            });
        }
    }

    fn save_remote(&mut self, edit: Edit, cx: &mut Context<Self>) {
        self.save_preference(Preference::Remote(edit), cx);
    }

    fn apply_remote_files(&mut self, cx: &mut Context<Self>) {
        let text = |input: &Option<Entity<SearchInput>>, cx: &App| {
            input
                .as_ref()
                .map(|input| input.read(cx).text().trim().to_owned())
                .filter(|text| !text.is_empty())
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
        };
        let (Some(cert), Some(key)) = (text(&self.remote.cert, cx), text(&self.remote.key, cx))
        else {
            self.remote.invalid = true;
            cx.notify();
            return;
        };
        self.remote.invalid = false;
        self.save_remote(Edit::Certificate(Certificate::Files { cert, key }), cx);
    }

    /// The device list's Revoke: the first click asks, the second sends.
    pub(in crate::settings_window) fn revoke_remote_device(
        &mut self,
        device: DeviceId,
        cx: &mut Context<Self>,
    ) {
        if self.remote.revoking.as_ref() != Some(&device) {
            self.remote.revoking = Some(device);
            cx.notify();
            return;
        }
        self.remote.revoking = None;
        let view = cx.entity().downgrade();
        Supervisor::revoke(cx, device, move |result, cx| {
            let _ = view.update(cx, |view, cx| {
                if let Err(error) = result {
                    view.error = Some(format!("Could not revoke the device: {error}"));
                }
                cx.notify();
            });
        });
        cx.notify();
    }

    /// The bind dropdown's choice at `index`.
    pub(in crate::settings_window) fn pick_remote_bind(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let health = Supervisor::health(cx);
        if let Some(bind) = bind_choices(self.config.remote.bind, &health).get(index) {
            self.save_remote(Edit::Bind(*bind), cx);
        }
    }

    pub(in crate::settings_window) fn render_remote_control(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let remote = &self.config.remote;
        let health = Supervisor::health(cx);
        let busy = Supervisor::busy(cx);
        let failure = Supervisor::failure(cx);
        let status = match &health {
            Health::Running(status) => Some(status),
            _ => None,
        };
        let enabled = remote.enabled;
        let mut switch_row = ui::row(theme, "remote-enabled", "Remote control")
            .description(match &failure {
                Some(error) => format!(
                    "Could not change the service: {}",
                    crate::notifications::safe_text(&error.to_string(), 300)
                ),
                None => health_line(&health, busy),
            })
            .switch(self.preference_toggle(
                "remote-enabled-toggle",
                enabled,
                Preference::Remote(Edit::Enabled(!enabled)),
                cx,
            ));
        if failure.is_some() {
            switch_row = switch_row.control(
                ui::button(theme, "remote-retry", "Retry", !busy)
                    .on_click(cx.listener(|_, _, _, cx| Supervisor::retry(cx))),
            );
        }
        let files = self.remote.files || matches!(remote.certificate, Certificate::Files { .. });
        let certificate_note = match (files, status) {
            (true, _) => "Your certificate and key, such as for your own domain.".to_owned(),
            (false, Some(status)) if !status.fingerprint.is_empty() => format!(
                "The daemon's own certificate authority, trusted once on the phone. CA SHA-256 {}",
                crate::notifications::safe_text(&status.fingerprint, 200)
            ),
            (false, _) => "The daemon's own certificate authority, trusted once on the phone. \
                           Its fingerprint shows while it runs."
                .to_owned(),
        };
        div()
            .flex()
            .flex_col()
            .child(ui::section(theme, "Service", None))
            .child(switch_row)
            .child(
                ui::row(
                    theme,
                    "remote-keep-running",
                    "Keep running when Herdr UI quits",
                )
                .description(
                    "Phones keep working and notifications keep coming. Pairing a new \
                         device still needs Herdr UI.",
                )
                .switch(self.preference_toggle(
                    "remote-keep-running-toggle",
                    remote.keep_running,
                    Preference::Remote(Edit::KeepRunning(!remote.keep_running)),
                    cx,
                )),
            )
            .child(ui::section(theme, "Connection", None))
            .child(self.remote_number_row(
                NumberField::RemotePort,
                ("remote-port", "Port"),
                "The HTTPS port phones connect to.",
                f32::from(remote.port),
                "",
                cx,
            ))
            .child(self.remote_bind_row(&health, cx))
            .child(
                ui::row(theme, "remote-addresses", "Addresses").description(match status {
                    Some(status) if !status.addresses.is_empty() => {
                        crate::remote::addresses(&status.addresses)
                    }
                    Some(_) => "No network address found.".to_owned(),
                    None => "Shown while remote control runs.".to_owned(),
                }),
            )
            .child(
                ui::row(theme, "remote-certificate", "Certificate")
                    .description(certificate_note)
                    .control(ui::segmented(
                        theme,
                        "remote-certificate-control",
                        &[(false, "Own CA"), (true, "My files")],
                        Some(files),
                        !self.busy(),
                        on_value(cx, |this, files: bool, _, cx| {
                            this.remote.files = files;
                            if !files {
                                this.save_remote(Edit::Certificate(Certificate::Own), cx);
                            }
                            cx.notify();
                        }),
                    )),
            )
            .when(files, |page| page.child(self.remote_files(cx)))
            .child(ui::section(theme, "Notifications", None))
            .child(
                ui::row(theme, "remote-notifications", "Notification text")
                    .description(
                        "Generic says which chat needs you without its text; Excerpt adds a \
                         short excerpt of the message.",
                    )
                    .control(ui::segmented(
                        theme,
                        "remote-notifications-control",
                        &[(Privacy::Generic, "Generic"), (Privacy::Excerpt, "Excerpt")],
                        Some(remote.notifications),
                        !self.busy(),
                        on_value(cx, |this, privacy, _, cx| {
                            this.save_remote(Edit::Notifications(privacy), cx)
                        }),
                    )),
            )
            .child(ui::section(theme, "Devices", None))
            .child(self.remote_number_row(
                NumberField::RemoteIdleDays,
                ("remote-idle", "Forget idle devices after"),
                "A device that has not connected for this long is forgotten and must pair again.",
                f32::from(remote.device_idle_days),
                " days",
                cx,
            ))
            .child(
                ui::row(theme, "remote-pair", "Pair a device")
                    .description(if enabled {
                        "Shows a QR code to scan with the phone's camera; you approve the device \
                         in the main window."
                    } else {
                        "Turn remote control on first."
                    })
                    .control(
                        ui::button(
                            theme,
                            "remote-pair-button",
                            "Pair a device\u{2026}",
                            enabled,
                        )
                        .when(enabled, |button| {
                            button.on_click(cx.listener(|this, _, _, cx| this.pair_remote(cx)))
                        }),
                    ),
            )
            .child(self.remote_devices(cx))
    }

    fn remote_number_row(
        &self,
        field: NumberField,
        (id, title): (&'static str, &'static str),
        description: &'static str,
        value: f32,
        unit: &'static str,
        cx: &mut Context<Self>,
    ) -> ui::Row {
        let on_change = on_value(cx, move |this, change, _, cx| {
            this.commit_number(field, change, cx)
        });
        let view = cx.entity().downgrade();
        ui::row(&self.theme, id, title)
            .description(description)
            .control(ui::number(
                &self.theme,
                ui::Number {
                    id: field.selector(),
                    value,
                    range: field.range(),
                    unit,
                    slider: false,
                    enabled: true,
                },
                self.number_edit_for(field),
                on_change,
                move |window, cx| {
                    let _ = view.update(cx, |this, cx| {
                        this.begin_number_edit(field, value, window, cx)
                    });
                },
            ))
    }

    fn remote_bind_row(&self, health: &Health, cx: &mut Context<Self>) -> ui::Row {
        let field = DropdownField::RemoteBind;
        let bind = self.config.remote.bind;
        let choices = bind_choices(bind, health);
        let selected = choices
            .iter()
            .position(|choice| *choice == bind)
            .unwrap_or(0);
        let options: Vec<SharedString> = choices
            .into_iter()
            .map(|choice| bind_label(choice).into())
            .collect();
        let view = cx.entity().downgrade();
        let (opener, picker, dismisser) = (view.clone(), view.clone(), view);
        ui::row(&self.theme, "remote-bind", "Listen on")
            .description("Every interface, or only one network's address.")
            .control(ui::dropdown(
                &self.theme,
                field.selector(),
                bind_label(bind),
                false,
                true,
                self.dropdown.as_ref().filter(|open| open.field == field),
                move |window, cx| {
                    let options = options.clone();
                    let _ = opener.update(cx, |this, cx| {
                        this.open_dropdown(field, options, selected, window, cx)
                    });
                },
                move |index, window, cx| {
                    let _ = picker.update(cx, |this, cx| {
                        this.pick_dropdown(field, index, cx);
                        window.focus(&this.focus, cx);
                    });
                },
                move |window, cx| {
                    let _ = dismisser.update(cx, |this, cx| this.close_dropdown(window, cx));
                },
            ))
    }

    fn remote_files(&self, cx: &mut Context<Self>) -> Div {
        let field =
            |label: &'static str, selector: &'static str, input: &Option<Entity<SearchInput>>| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(div().w(px(80.)).flex_none().child(label))
                    .children(input.as_ref().map(|input| {
                        div()
                            .debug_selector(move || selector.into())
                            .flex_1()
                            .min_w_0()
                            .child(input.clone())
                    }))
            };
        self.control_card("remote-files")
            .child(field("Certificate", "remote-cert-path", &self.remote.cert))
            .child(field("Private key", "remote-key-path", &self.remote.key))
            .when(self.remote.invalid, |card| {
                card.child(self.control_note("Give both files as absolute paths."))
            })
            .child(
                ui::button(
                    &self.theme,
                    "remote-files-apply",
                    "Use these files",
                    !self.busy(),
                )
                .when(!self.busy(), |button| {
                    button.on_click(cx.listener(|this, _, _, cx| this.apply_remote_files(cx)))
                }),
            )
    }

    fn remote_devices(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let devices = Supervisor::devices(cx)
            .map(<[Device]>::to_vec)
            .unwrap_or_default();
        let now = crate::remote::now_ms();
        let rows =
            devices.iter().enumerate().map(|(index, device)| {
                let id = device.id.clone();
                let asking = self.remote.revoking.as_ref() == Some(&device.id);
                let cancel = div()
                    .id(("remote-device-cancel", index))
                    .debug_selector(move || format!("remote-device-{index}-cancel"))
                    .role(Role::Button)
                    .px(px(8.))
                    .cursor_pointer()
                    .child("Cancel")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.remote.revoking = None;
                        cx.notify();
                    }));
                ui::row(
                    theme,
                    format!("remote-device-{index}"),
                    crate::notifications::safe_text(&device.name, 64),
                )
                .description(if asking {
                    "Revoke it? Its sessions end and its notifications stop at once.".to_owned()
                } else {
                    device_line(device, now)
                })
                .when(asking, |row| row.control(cancel))
                .control(
                    div()
                        .id(("remote-device-revoke", index))
                        .debug_selector(move || format!("remote-device-{index}-revoke"))
                        .role(Role::Button)
                        .aria_label(if asking { "Confirm revoke" } else { "Revoke" })
                        .px(px(10.))
                        .py(px(3.))
                        .rounded(px(crate::config::corners::CONTROL))
                        .border_1()
                        .border_color(rgb(theme.active))
                        .text_color(crate::menu::danger(theme))
                        .cursor_pointer()
                        .hover(|button| button.bg(rgb(theme.active)))
                        .child(if asking { "Revoke" } else { "Revoke\u{2026}" })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.revoke_remote_device(id.clone(), cx)
                        })),
                )
                .into_any_element()
            });
        let summary = match (self.config.remote.enabled, devices.len()) {
            (false, _) => "Shown while remote control runs.".to_owned(),
            (true, 0) => "No device is paired yet.".to_owned(),
            (true, 1) => "1 device can control your chats.".to_owned(),
            (true, count) => format!("{count} devices can control your chats."),
        };
        div()
            .flex()
            .flex_col()
            .child(ui::row(theme, "remote-devices", "Paired devices").description(summary))
            .children(rows)
    }

    /// Opens the pairing dialog in the window these settings came from.
    fn pair_remote(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.source.upgrade() else {
            self.error = Some("Open a Herdr UI window to pair a device.".into());
            cx.notify();
            return;
        };
        let target = source.entity_id();
        for handle in cx.windows() {
            let Some(handle) = handle.downcast::<crate::HerdrWindow>() else {
                continue;
            };
            let _ = handle.update(cx, |view, window, cx| {
                if cx.entity_id() != target {
                    return;
                }
                window.activate_window();
                if !view.open_remote_pairing(window, cx) {
                    view.local_error = Some("Close the open dialog, then pair the device.".into());
                    cx.notify();
                }
            });
        }
    }
}
