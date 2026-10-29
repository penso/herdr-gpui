//! The Container tab of Settings > Cloud devices. Its fields save to
//! `[container]` in the local config. Creating a container hands a job to the
//! main window, which shows progress in the device picker and a toast when it
//! is ready. Engine reads, stops, and removals run through `cloud::worker`,
//! never on the UI thread.
// i18n: migrated

use super::{
    super::{Section, SettingsWindow},
    Hint, follow_config, input,
};
use crate::{
    cloud::{
        CloudProvider,
        worker::{self, Worker},
    },
    config::{Config, ContainerFields},
    container::{
        Settings,
        setup::{self, AddRequest, Device, Overview, State},
    },
    i18n::{self, fl},
    search_input::SearchInput,
};
use gpui::{prelude::*, *};

/// The editable `[container]` keys, in display order, with their labels'
/// message IDs and hints.
const FIELDS: [(&str, Hint); 5] = [
    (
        "settings-cloud-container-engine",
        Hint::Message("settings-cloud-container-engine-placeholder"),
    ),
    (
        "settings-cloud-container-image",
        Hint::Value(setup::DEFAULT_IMAGE),
    ),
    (
        "settings-cloud-container-cpus",
        Hint::Value(setup::DEFAULT_CPUS),
    ),
    (
        "settings-cloud-container-memory",
        Hint::Value(setup::DEFAULT_MEMORY),
    ),
    (
        "settings-cloud-container-auto-stop",
        Hint::Message("settings-cloud-container-auto-stop-placeholder"),
    ),
];

enum Update {
    Overview(crate::Result<Overview>),
    Done(crate::Result<()>),
}

pub(in crate::settings_window) struct ContainerCard {
    fields: [Entity<SearchInput>; 5],
    /// The config values the fields last loaded; see `follow_config`.
    loaded: ContainerFields,
    /// `None` until the first read finishes.
    overview: Option<Overview>,
    /// The user's approval to install Herdr in containers created here.
    install: bool,
    /// Run the setup script on the next device made here, when one is set.
    setup: bool,
    message: Option<String>,
    job: Option<Worker>,
    /// Tests build the card without asking the user's real engine.
    #[cfg(test)]
    reads_engine: bool,
}

impl ContainerCard {
    fn new(
        config: &Config,
        theme: &crate::config::Theme,
        cx: &mut Context<SettingsWindow>,
    ) -> Self {
        let loaded = ContainerFields::from_config(&config.container);
        let texts = texts(&loaded);
        let fields = std::array::from_fn(|index| {
            let hint = FIELDS[index].1;
            input(move || hint.text(), texts[index], config, theme, cx)
        });
        Self {
            fields,
            loaded,
            overview: None,
            install: true,
            setup: true,
            message: None,
            job: None,
            #[cfg(test)]
            reads_engine: false,
        }
    }

    fn values(&self, cx: &App) -> ContainerFields {
        let text = |index: usize| self.fields[index].read(cx).text().trim().to_owned();
        ContainerFields {
            engine: text(0),
            image: text(1),
            cpus: text(2),
            memory: text(3),
            auto_stop: text(4),
        }
    }

    fn devices(&self) -> &[Device] {
        self.overview
            .as_ref()
            .map_or(&[], |overview| &overview.devices)
    }
}

/// The editable values in `FIELDS` order.
fn texts(values: &ContainerFields) -> [&str; 5] {
    [
        &values.engine,
        &values.image,
        &values.cpus,
        &values.memory,
        &values.auto_stop,
    ]
}

/// What the row says about a container: its engine and its state.
fn state_text(engine: &str, state: Option<State>) -> String {
    let engine = engine.to_owned();
    match state {
        Some(State::Running) => fl!("settings-cloud-container-running", engine = engine),
        Some(State::Stopped) => fl!("settings-cloud-container-stopped", engine = engine),
        Some(State::Missing) => fl!("settings-cloud-container-missing", engine = engine),
        None => fl!("settings-cloud-container-unreachable", engine = engine),
    }
}

impl SettingsWindow {
    pub(super) fn open_container_card(&mut self, cx: &mut Context<Self>) {
        if self.container_card.is_none() {
            self.container_card = Some(ContainerCard::new(&self.config, &self.theme, cx));
        }
        self.refresh_container(cx);
    }

    fn container_settings(&self) -> crate::container::Result<Settings> {
        Settings::resolve(&self.config.container)
    }

    fn container_job(
        &mut self,
        cx: &mut Context<Self>,
        work: impl FnOnce(&dyn Fn() -> bool, &dyn Fn(Update)) + Send + 'static,
    ) {
        let Some(card) = &mut self.container_card else {
            return;
        };
        card.job = None;
        match worker::spawn(
            "herdr-container-settings",
            cx,
            work,
            |this: &mut Self, update, cx| this.apply_container(update, cx),
        ) {
            Ok(job) => {
                if let Some(card) = &mut self.container_card {
                    card.job = Some(job);
                }
            }
            Err(error) => {
                tracing::error!(category = "container_worker", error_kind = ?error.kind(), "Could not start the container settings worker");
                if let Some(card) = &mut self.container_card {
                    // i18n: data
                    card.message = Some(crate::cloud::Error::Worker("settings").to_string());
                }
            }
        }
    }

    fn refresh_container(&mut self, cx: &mut Context<Self>) {
        self.watch_sandbox_images(cx);
        #[cfg(test)]
        if self
            .container_card
            .as_ref()
            .is_some_and(|card| !card.reads_engine)
        {
            return;
        }
        let settings = match self.container_settings() {
            Ok(settings) => settings,
            Err(error) => {
                if let Some(card) = &mut self.container_card {
                    card.message = Some(error.to_string());
                }
                return;
            }
        };
        self.container_job(cx, move |cancelled, send| {
            send(Update::Overview(
                setup::overview(&settings, cancelled).map_err(Into::into),
            ));
        });
    }

    fn apply_container(&mut self, update: Update, cx: &mut Context<Self>) {
        let Some(card) = &mut self.container_card else {
            return;
        };
        match update {
            Update::Overview(Ok(overview)) => card.overview = Some(overview),
            Update::Overview(Err(error)) => card.message = Some(error.to_string()),
            Update::Done(result) => {
                card.message = result.err().map(|error| error.to_string());
                self.refresh_container(cx);
            }
        }
    }

    fn save_container(&mut self, cx: &mut Context<Self>) {
        let Some(card) = &mut self.container_card else {
            return;
        };
        let values = card.values(cx);
        let existing = self.config.container.clone();
        card.message = None;
        // Reloading the saved config refreshes the engine line.
        self.save_native(move || Config::save_container(&values, &existing), cx);
    }

    /// Forget the device; the container keeps running on the engine.
    fn forget_container_device(&mut self, id: String, cx: &mut Context<Self>) {
        self.container_job(cx, move |_, send| {
            send(Update::Done(
                crate::cloud::remove(CloudProvider::Container, &id).map_err(Into::into),
            ));
        });
    }

    /// Save a container this app made, but that is not a device, as one.
    fn adopt_container(&mut self, id: String, cx: &mut Context<Self>) {
        let Some((Ok((engine, _)), install)) = self.container_card.as_ref().and_then(|card| {
            card.overview
                .as_ref()
                .map(|overview| (overview.engine.clone(), card.install))
        }) else {
            return;
        };
        self.container_job(cx, move |cancelled, send| {
            send(Update::Done(
                setup::adopt(engine, &id, install, cancelled)
                    .map(drop)
                    .map_err(Into::into),
            ));
        });
    }

    fn stop_container(&mut self, id: String, cx: &mut Context<Self>) {
        self.container_job(cx, move |_, send| {
            send(Update::Done(setup::stop_saved(&id).map_err(Into::into)));
        });
    }

    /// Hand a new container to the main window, which runs and reports the
    /// job so it survives this window closing.
    fn create_container(&mut self, cx: &mut Context<Self>) {
        let settings = match self.container_settings() {
            Ok(settings) => settings,
            Err(error) => {
                if let Some(card) = &mut self.container_card {
                    card.message = Some(error.to_string());
                }
                cx.notify();
                return;
            }
        };
        let Some(card) = &mut self.container_card else {
            return;
        };
        let name = setup::random_name();
        let request = AddRequest {
            settings,
            name: name.clone(),
            install: card.install,
        };
        let run = card.setup;
        let script = self.cloud_setup_script(CloudProvider::Container, run);
        let started = self.source.update(cx, |window, cx| {
            window.start_cloud_job(
                CloudProvider::Container,
                name.clone(),
                script,
                move |cancelled, report| setup::add_device(request, cancelled, report),
                cx,
            )
        });
        if let Some(card) = &mut self.container_card {
            card.message = Some(match started {
                Ok(Ok(())) => fl!("settings-cloud-creating", name = name),
                Ok(Err(error)) => error.to_string(),
                Err(_) => fl!("settings-cloud-container-no-window"),
            });
        }
        cx.notify();
    }

    pub(super) fn container_config_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(card) = &mut self.container_card {
            let loaded = ContainerFields::from_config(&self.config.container);
            follow_config(&card.fields, texts(&card.loaded), texts(&loaded), cx);
            card.loaded = loaded;
        }
        if self.container_card.is_some() && self.section == Section::CloudDevices {
            self.refresh_container(cx);
        }
    }

    pub(super) fn render_container_card(&self, cx: &mut Context<Self>) -> Div {
        let Some(card) = &self.container_card else {
            return div();
        };
        let ready = !self.busy();
        let status = match card.overview.as_ref().map(|overview| &overview.engine) {
            None => fl!("settings-cloud-container-checking"),
            Some(Ok((engine, version))) => fl!(
                "settings-cloud-container-engine-running",
                engine = engine.name(),
                version = version.clone()
            ),
            Some(Err(error)) => error.clone(),
        };
        let usable = card
            .overview
            .as_ref()
            .is_some_and(|overview| overview.engine.is_ok());
        let mut engine = self
            .cloud_card(crate::cloud::name(CloudProvider::Container).to_owned())
            .debug_selector(|| "cloud-container-card".into())
            .child(self.control_note(fl!("settings-cloud-container-note")))
            .child(div().min_w_0().child(status));
        for (index, (label, _)) in FIELDS.iter().enumerate() {
            engine = engine.child(
                self.cloud_field(i18n::message(label, &[]), card.fields[index].clone().into()),
            );
        }
        engine = engine.child(
            div().flex().justify_end().child(
                self.cloud_button("cloud-container-save", fl!("settings-cloud-save"), ready)
                    .debug_selector(|| "cloud-container-save".into())
                    .when(ready, |button| {
                        button.on_click(cx.listener(|this, _, _, cx| this.save_container(cx)))
                    }),
            ),
        );
        let create = ready && usable && crate::cloud::unavailable().is_none();
        let mut new = self
            .cloud_card(fl!("settings-cloud-container-new"))
            .debug_selector(|| "cloud-container-new".into())
            .child(self.control_note(fl!("settings-cloud-container-new-note")))
            .child(
                self.control_switch(
                    "cloud-container-install",
                    fl!("settings-cloud-container-install"),
                    card.install,
                    ready,
                )
                .when(ready, |row| {
                    row.on_click(cx.listener(|this, _, _, cx| {
                        if let Some(card) = &mut this.container_card {
                            card.install = !card.install;
                        }
                        cx.notify();
                    }))
                }),
            )
            .children(self.cloud_setup_switch(
                CloudProvider::Container,
                card.setup,
                ready,
                |this| {
                    if let Some(card) = &mut this.container_card {
                        card.setup = !card.setup;
                    }
                },
                cx,
            ))
            .child(
                div().flex().justify_end().child(
                    self.cloud_button(
                        "cloud-container-create",
                        fl!("settings-cloud-container-create"),
                        create,
                    )
                    .debug_selector(|| "cloud-container-create".into())
                    .when(create, |button| {
                        button.on_click(cx.listener(|this, _, _, cx| this.create_container(cx)))
                    }),
                ),
            );
        if let Some(message) = &card.message {
            new = new.child(div().min_w_0().child(message.clone()));
        }
        let mut devices = self
            .cloud_card(fl!("settings-cloud-container-devices"))
            .debug_selector(|| "cloud-container-devices".into())
            .child(self.control_note(fl!("settings-cloud-container-devices-note")));
        if card.devices().is_empty() {
            devices = devices.child(self.control_note(fl!("settings-cloud-none-yet")));
        }
        for (index, device) in card.devices().iter().enumerate() {
            let running = device.state == Some(State::Running);
            let id = device.saved.id.clone();
            let stop = self
                .cloud_button(
                    SharedString::from(format!("cloud-container-stop-{index}")),
                    fl!("settings-cloud-container-stop"),
                    ready && running,
                )
                .debug_selector(move || format!("cloud-container-stop-{index}"))
                .when(ready && running, |button| {
                    button.on_click(
                        cx.listener(move |this, _, _, cx| this.stop_container(id.clone(), cx)),
                    )
                })
                .into_any_element();
            devices = devices.child(self.cloud_device_row_with(
                index,
                &device.saved,
                state_text(&device.saved.account, device.state),
                ready,
                Self::forget_container_device,
                [stop],
                cx,
            ));
        }
        let unsaved = card
            .overview
            .as_ref()
            .map_or(&[][..], |overview| &overview.unsaved);
        let unsaved = (!unsaved.is_empty()).then(|| {
            let mut list = self
                .cloud_card(fl!("settings-cloud-container-unsaved"))
                .debug_selector(|| "cloud-container-unsaved".into())
                .child(self.control_note(fl!("settings-cloud-container-unsaved-note")));
            for (index, name) in unsaved.iter().enumerate() {
                let id = name.clone();
                list = list.child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(12.))
                        .child(div().min_w_0().truncate().child(name.clone()))
                        .child(
                            self.cloud_button(
                                SharedString::from(format!("cloud-container-adopt-{index}")),
                                fl!("settings-cloud-container-adopt"),
                                ready,
                            )
                            .debug_selector(move || format!("cloud-container-adopt-{index}"))
                            .when(ready, |button| {
                                button.on_click(cx.listener(move |this, _, _, cx| {
                                    this.adopt_container(id.clone(), cx)
                                }))
                            }),
                        ),
                );
            }
            list
        });
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .min_w_0()
            .child(engine)
            .child(new)
            .child(devices)
            .children(unsaved)
            .child(self.render_sandbox_images(CloudProvider::Container, cx))
    }
}

#[cfg(test)]
mod tests;
