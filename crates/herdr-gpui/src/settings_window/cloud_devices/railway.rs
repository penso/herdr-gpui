//! The Railway tab of Settings > Cloud devices. There is no account: the tab is
//! about the SSH key. Choose one the user already has or generate a named one
//! (which fills the path), then set up the VM (installing Herdr after
//! approval) and save the device with that key. Setup runs as a job in the main window, which shows progress in the
//! device picker; reads and removals run through `cloud::worker`.
// i18n: migrated

use super::super::{Section, SettingsWindow};
use super::input;
use crate::{
    cloud::{
        CloudProvider,
        worker::{self, Worker},
    },
    i18n::fl,
    railway::setup::{self, AddRequest, Overview},
    search_input::SearchInput,
};
use gpui::{prelude::*, *};

/// Where the key for a new device comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeySource {
    Generate,
    Existing,
}

enum Update {
    Overview(crate::railway::Result<Overview>),
    /// A key was generated at this path.
    Generated(crate::railway::Result<std::path::PathBuf>),
    Done(crate::railway::Result<()>),
}

pub(in crate::settings_window) struct RailwayCard {
    path: Entity<SearchInput>,
    /// The name for a key to generate.
    name: Entity<SearchInput>,
    source: KeySource,
    /// The user's approval to install Herdr on VMs set up from here.
    install: bool,
    /// Run the setup script on the next device made here, when one is set.
    setup: bool,
    /// `None` until the first read finishes.
    overview: Option<Overview>,
    message: Option<String>,
    job: Option<Worker>,
    /// A generate or remove job is running. Its worker is cancelled when
    /// dropped, so nothing else may replace it until it reports.
    changing: bool,
    /// Tests build the card without the user's real saved devices.
    #[cfg(test)]
    reads_disk: bool,
}

impl RailwayCard {
    fn new(
        config: &crate::config::Config,
        theme: &crate::config::Theme,
        cx: &mut Context<SettingsWindow>,
    ) -> Self {
        Self {
            // An example path.
            path: input(
                // i18n: data
                || "/Users/you/.ssh/id_ed25519".to_owned(),
                "",
                config,
                theme,
                cx,
            ),
            name: input(
                || "herdr-railway".to_owned(),
                "herdr-railway",
                config,
                theme,
                cx,
            ),
            source: KeySource::Generate,
            install: true,
            setup: true,
            overview: None,
            message: None,
            job: None,
            changing: false,
            #[cfg(test)]
            reads_disk: false,
        }
    }

    fn devices(&self) -> &[setup::Device] {
        self.overview
            .as_ref()
            .map_or(&[], |overview| &overview.devices)
    }
}

/// Where a key named `name` will be created.
fn location_note(directory: &crate::railway::KeyDirectory, name: &str) -> String {
    let folder = directory.path.display();
    let (folder, name) = (folder.to_string(), name.to_owned());
    if directory.user {
        fl!(
            "settings-cloud-railway-key-location",
            folder = folder,
            name = name
        )
    } else {
        fl!(
            "settings-cloud-railway-key-location-no-ssh",
            folder = folder,
            name = name
        )
    }
}

/// Settings can move between main windows while a setup stays with its owner.
fn railway_setup_running(cx: &App) -> bool {
    cx.windows().into_iter().any(|window| {
        window
            .downcast::<crate::HerdrWindow>()
            .is_some_and(|window| {
                window
                    .read(cx)
                    .is_ok_and(|window| window.cloud_jobs.has(CloudProvider::Railway))
            })
    })
}

impl SettingsWindow {
    pub(super) fn open_railway_card(&mut self, cx: &mut Context<Self>) {
        if self.railway_card.is_none() {
            self.railway_card = Some(RailwayCard::new(&self.config, &self.theme, cx));
        }
        self.refresh_railway(cx);
    }

    fn railway_job(
        &mut self,
        cx: &mut Context<Self>,
        work: impl FnOnce(&dyn Fn() -> bool, &dyn Fn(Update)) + Send + 'static,
    ) {
        let Some(card) = &mut self.railway_card else {
            return;
        };
        card.job = None;
        match worker::spawn(
            "herdr-railway-settings",
            cx,
            work,
            |this: &mut Self, update, cx| this.apply_railway(update, cx),
        ) {
            Ok(job) => {
                if let Some(card) = &mut self.railway_card {
                    card.job = Some(job);
                }
            }
            Err(error) => {
                tracing::error!(category = "railway_worker", error_kind = ?error.kind(), "Could not start Railway settings worker");
                if let Some(card) = &mut self.railway_card {
                    card.changing = false;
                    // i18n: data
                    card.message = Some(crate::cloud::Error::Worker("settings").to_string());
                }
            }
        }
    }

    fn refresh_railway(&mut self, cx: &mut Context<Self>) {
        // A read must not replace, and so cancel, a change in progress; the
        // change refreshes when it reports.
        if self.railway_card.as_ref().is_some_and(|card| card.changing) {
            return;
        }
        #[cfg(test)]
        if self
            .railway_card
            .as_ref()
            .is_some_and(|card| !card.reads_disk)
        {
            return;
        }
        self.railway_job(cx, |_, send| send(Update::Overview(setup::overview())));
    }

    fn apply_railway(&mut self, update: Update, cx: &mut Context<Self>) {
        let Some(card) = &mut self.railway_card else {
            return;
        };
        match update {
            Update::Overview(Ok(overview)) => card.overview = Some(overview),
            Update::Overview(Err(error)) => card.message = Some(error.to_string()),
            Update::Generated(result) => {
                card.changing = false;
                card.message = match result {
                    Ok(path) => {
                        // The new key is the one to set up with.
                        card.source = KeySource::Existing;
                        card.path.update(cx, |input, cx| {
                            input.set_text_selected(&path.to_string_lossy(), cx)
                        });
                        Some(fl!(
                            "settings-cloud-railway-key-created",
                            path = path.display().to_string()
                        ))
                    }
                    Err(error) => Some(error.to_string()),
                };
                self.refresh_railway(cx);
            }
            Update::Done(result) => {
                card.changing = false;
                card.message = result.err().map(|error| error.to_string());
                self.refresh_railway(cx);
            }
        }
    }

    pub(super) fn railway_config_changed(&mut self, cx: &mut Context<Self>) {
        if self.railway_card.is_some() && self.section == Section::CloudDevices {
            self.refresh_railway(cx);
        }
    }

    /// Start a generate or remove job unless one is already running. A device
    /// is not removed while a setup runs, which would save it again.
    fn change_railway(
        &mut self,
        cx: &mut Context<Self>,
        work: impl FnOnce(&dyn Fn() -> bool, &dyn Fn(Update)) + Send + 'static,
    ) {
        let setting_up = railway_setup_running(cx);
        let Some(card) = &mut self.railway_card else {
            return;
        };
        if card.changing || setting_up {
            card.message = Some(fl!("settings-cloud-railway-busy"));
            cx.notify();
            return;
        }
        card.changing = true;
        self.railway_job(cx, work);
    }

    /// Forget the device. Its key files are left alone.
    fn forget_railway_device(&mut self, id: String, cx: &mut Context<Self>) {
        self.change_railway(cx, move |_, send| {
            send(Update::Done(setup::forget_device(&id)));
        });
    }

    /// Make the key named in the field. Nothing is created at Railway.
    fn generate_railway_key(&mut self, cx: &mut Context<Self>) {
        let Some(card) = &mut self.railway_card else {
            return;
        };
        let name = card.name.read(cx).text().trim().to_owned();
        if crate::railway::valid_name(&name).is_err() {
            card.message = Some(fl!("settings-cloud-railway-name-rule"));
            cx.notify();
            return;
        }
        card.message = None;
        self.change_railway(cx, move |cancelled, send| {
            send(Update::Generated(setup::generate_key(&name, cancelled)));
        });
    }

    /// The system file dialog; the chosen path fills the field, and nothing is
    /// read from it until setup runs.
    fn browse_railway_key(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(fl!("settings-cloud-railway-choose-key").into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |this, cx| {
                if let Some(card) = &mut this.railway_card {
                    card.path.update(cx, |input, cx| {
                        input.set_text_selected(&path.to_string_lossy(), cx)
                    });
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Hand a setup to the main window, which runs and reports the job so it
    /// survives this window closing. Nothing else starts one.
    fn setup_railway(&mut self, cx: &mut Context<Self>) {
        let setting_up = railway_setup_running(cx);
        let Some(card) = &mut self.railway_card else {
            return;
        };
        if card.changing || setting_up {
            card.message = Some(fl!("settings-cloud-railway-busy"));
            cx.notify();
            return;
        }
        let text = card.path.read(cx).text().trim().to_owned();
        if card.source != KeySource::Existing || text.is_empty() {
            card.message = Some(fl!("settings-cloud-railway-key-needed"));
            cx.notify();
            return;
        }
        let name = setup::random_name();
        let request = AddRequest {
            key: setup::expand(&text),
            name: name.clone(),
            install: card.install,
        };
        let run = card.setup;
        let script = self.cloud_setup_script(CloudProvider::Railway, run);
        let started = self.source.update(cx, |window, cx| {
            window.start_cloud_job(
                CloudProvider::Railway,
                name.clone(),
                script,
                move |cancelled, report| setup::add_device(request, cancelled, report),
                cx,
            )
        });
        if let Some(card) = &mut self.railway_card {
            card.message = Some(match started {
                Ok(Ok(())) => fl!("settings-cloud-railway-connecting", name = name),
                Ok(Err(error)) => error.to_string(),
                Err(_) => fl!("settings-cloud-railway-no-window"),
            });
        }
        cx.notify();
    }

    fn railway_button(
        &self,
        id: &'static str,
        label: String,
        enabled: bool,
        on_click: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.cloud_button(id, label, enabled)
            .debug_selector(move || id.to_owned())
            .when(enabled, |button| {
                button.on_click(cx.listener(move |this, _, _, cx| on_click(this, cx)))
            })
    }

    pub(super) fn render_railway_card(&self, cx: &mut Context<Self>) -> Div {
        let Some(card) = &self.railway_card else {
            return div();
        };
        let ready = !self.busy();
        let available = ready && crate::cloud::unavailable().is_none();
        let overview = card.overview.as_ref();
        let mut about = self
            .cloud_card(crate::cloud::name(CloudProvider::Railway).to_owned())
            .debug_selector(|| "cloud-railway-card".into())
            .child(self.control_note(fl!("settings-cloud-railway-note")));
        if crate::cloud::unavailable().is_some() {
            about = about.child(self.control_note(fl!("settings-cloud-railway-unavailable")));
        }
        let mut key = self
            .cloud_card(fl!("settings-cloud-railway-key"))
            .debug_selector(|| "cloud-railway-key".into())
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap(px(8.))
                    .child(self.railway_choice(
                        "cloud-railway-source-generate",
                        fl!("settings-cloud-railway-source-generate"),
                        KeySource::Generate,
                        card.source,
                        ready,
                        cx,
                    ))
                    .child(self.railway_choice(
                        "cloud-railway-source-existing",
                        fl!("settings-cloud-railway-source-existing"),
                        KeySource::Existing,
                        card.source,
                        ready,
                        cx,
                    )),
            );
        key = match card.source {
            KeySource::Generate => {
                let name = card.name.read(cx).text().trim().to_owned();
                let valid = crate::railway::valid_name(&name).is_ok();
                let note = match overview {
                    _ if !valid => fl!("settings-cloud-railway-name-rule"),
                    Some(overview) => location_note(&overview.new_key_directory, &name),
                    None => fl!("settings-cloud-railway-key-location-checking"),
                };
                key.child(self.cloud_field(
                    fl!("settings-cloud-railway-key-name"),
                    card.name.clone().into(),
                ))
                .child(self.control_note(note))
                .child(div().flex().justify_end().child(self.railway_button(
                    "cloud-railway-generate",
                    fl!("settings-cloud-railway-generate"),
                    available && valid && overview.is_some(),
                    |this, cx| this.generate_railway_key(cx),
                    cx,
                )))
            }
            KeySource::Existing => key
                .child(self.cloud_field(
                    fl!("settings-cloud-railway-key-path"),
                    card.path.clone().into(),
                ))
                .child(self.control_note(fl!("settings-cloud-railway-key-path-note")))
                .child(self.railway_button(
                    "cloud-railway-browse",
                    fl!("settings-cloud-railway-browse"),
                    ready,
                    |this, cx| this.browse_railway_key(cx),
                    cx,
                )),
        };
        key = key
            .child(
                self.control_switch(
                    "cloud-railway-install",
                    fl!("settings-cloud-railway-install"),
                    card.install,
                    ready,
                )
                .when(ready, |row| {
                    row.on_click(cx.listener(|this, _, _, cx| {
                        if let Some(card) = &mut this.railway_card {
                            card.install = !card.install;
                        }
                        cx.notify();
                    }))
                }),
            )
            .children(self.cloud_setup_switch(
                CloudProvider::Railway,
                card.setup,
                ready,
                |this| {
                    if let Some(card) = &mut this.railway_card {
                        card.setup = !card.setup;
                    }
                },
                cx,
            ))
            .child(div().flex().justify_end().child(self.railway_button(
                "cloud-railway-create",
                fl!("settings-cloud-railway-create"),
                available,
                |this, cx| this.setup_railway(cx),
                cx,
            )));
        if let Some(message) = &card.message {
            key = key.child(div().min_w_0().child(message.clone()));
        }
        let mut devices = self
            .cloud_card(fl!("settings-cloud-railway-devices"))
            .debug_selector(|| "cloud-railway-devices".into());
        if card.devices().is_empty() {
            devices = devices.child(self.control_note(fl!("settings-cloud-none-yet")));
        }
        for (index, device) in card.devices().iter().enumerate() {
            let (machine, key) = (
                device.saved.machine.clone(),
                device.key_path().display().to_string(),
            );
            let detail = if device.key_present {
                fl!(
                    "settings-cloud-railway-device",
                    machine = machine,
                    key = key
                )
            } else {
                fl!(
                    "settings-cloud-railway-device-key-missing",
                    machine = machine,
                    key = key
                )
            };
            devices = devices.child(self.cloud_device_row(
                index,
                &device.saved,
                detail,
                ready,
                |this, id, cx| this.forget_railway_device(id, cx),
                cx,
            ));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .min_w_0()
            .child(about)
            .child(key)
            .child(devices)
    }

    fn railway_choice(
        &self,
        id: &'static str,
        label: String,
        source: KeySource,
        current: KeySource,
        enabled: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.control_choice(id, label, current == source, enabled)
            .debug_selector(move || id.to_owned())
            .when(enabled, |choice| {
                choice.on_click(cx.listener(move |this, _, _, cx| {
                    if let Some(card) = &mut this.railway_card {
                        card.source = source;
                    }
                    cx.notify();
                }))
            })
    }
}

#[cfg(test)]
mod tests;
