//! The boxd tab of Settings > Cloud devices. Its API URL saves to `[boxd]`
//! in the local config; its API key saves to the credential store. Creating a
//! machine hands a job to the main window, which shows progress in
//! the device picker and a toast when it is ready. Account reads and removals
//! run through `cloud::worker`, never on the UI thread.
// i18n: migrated

use super::{
    super::{Section, SettingsWindow},
    follow_config, input,
};
use crate::direction::Logical;
use crate::{
    boxd::{
        Settings,
        setup::{self, AddRequest, Key, Overview},
    },
    cloud::{
        CloudProvider, SavedDevice,
        worker::{self, Worker},
    },
    config::{BoxdFields, Config},
    github::Store,
    i18n::{Localize, fl},
    search_input::SearchInput,
};
use gpui::{prelude::*, *};
use secrecy::SecretString;

enum Update {
    Overview(crate::boxd::Result<Overview>),
    Done(crate::boxd::Result<()>),
}

/// The account line, while a job may be changing it.
enum Account {
    Checking,
    Known(Overview),
}

pub(in crate::settings_window) struct BoxdCard {
    api_url: [Entity<SearchInput>; 1],
    /// The config values the field last loaded; see `follow_config`.
    loaded: BoxdFields,
    key: Entity<SearchInput>,
    account: Account,
    /// The user's approval to install Herdr in machines created from here.
    install: bool,
    /// Run the setup script on the next device made here, when one is set.
    setup: bool,
    message: Option<String>,
    job: Option<Worker>,
}

/// The values the field shows for `config`. boxd's own service is the usual
/// account, so an unset API URL shows it; saving it sets boxd up.
fn loaded(config: &Config) -> BoxdFields {
    let mut values = BoxdFields::from_config(&config.boxd);
    if values.api_url.is_empty() {
        values.api_url = crate::boxd::DEFAULT_API_URL.into();
    }
    values
}

impl BoxdCard {
    fn new(
        config: &Config,
        theme: &crate::config::Theme,
        cx: &mut Context<SettingsWindow>,
    ) -> Self {
        let loaded = loaded(config);
        let api_url = [input(
            || crate::boxd::DEFAULT_API_URL.to_owned(),
            &loaded.api_url,
            config,
            theme,
            cx,
        )];
        let key = input(
            || fl!("settings-cloud-boxd-key-placeholder"),
            "",
            config,
            theme,
            cx,
        );
        key.update(cx, |input, cx| input.set_masked(true, cx));
        Self {
            api_url,
            loaded,
            key,
            account: Account::Checking,
            install: true,
            setup: true,
            message: None,
            job: None,
        }
    }

    fn values(&self, cx: &App) -> BoxdFields {
        BoxdFields {
            api_url: self.api_url[0].read(cx).text().trim().to_owned(),
        }
    }

    fn devices(&self) -> &[SavedDevice] {
        match &self.account {
            Account::Known(overview) => &overview.devices,
            Account::Checking => &[],
        }
    }
}

impl SettingsWindow {
    pub(super) fn open_boxd_card(&mut self, cx: &mut Context<Self>) {
        if self.boxd_card.is_none() {
            self.boxd_card = Some(BoxdCard::new(&self.config, &self.theme, cx));
        }
        self.refresh_boxd(cx);
    }

    fn boxd_settings(&self) -> Option<Settings> {
        self.config.boxd.settings().ok().flatten()
    }

    fn boxd_job(
        &mut self,
        cx: &mut Context<Self>,
        work: impl FnOnce(&dyn Fn() -> bool, &dyn Fn(Update)) + Send + 'static,
    ) {
        let Some(card) = &mut self.boxd_card else {
            return;
        };
        card.job = None;
        match worker::spawn(
            "herdr-boxd-settings",
            cx,
            work,
            |this: &mut Self, update, cx| this.apply_boxd(update, cx),
        ) {
            Ok(job) => {
                if let Some(card) = &mut self.boxd_card {
                    card.job = Some(job);
                }
            }
            Err(error) => {
                tracing::error!(category = "boxd_worker", error_kind = ?error.kind(), "Could not start the boxd settings worker");
                if let Some(card) = &mut self.boxd_card {
                    // i18n: data
                    card.message = Some(
                        crate::cloud::Error::Worker(crate::cloud::Worker::Settings).localize(),
                    );
                }
            }
        }
    }

    fn boxd_missing_overview() -> Overview {
        Overview {
            key: Key::Missing,
            machines: None,
            devices: Vec::new(),
        }
    }

    fn refresh_boxd(&mut self, cx: &mut Context<Self>) {
        let Some(settings) = self.boxd_settings() else {
            if let Some(card) = &mut self.boxd_card {
                card.job = None;
                card.account = Account::Known(Self::boxd_missing_overview());
            }
            return;
        };
        if let Some(card) = &mut self.boxd_card {
            card.account = Account::Checking;
        }
        self.boxd_job(cx, move |_, send| {
            send(Update::Overview(setup::overview(&settings)));
        });
    }

    fn apply_boxd(&mut self, update: Update, cx: &mut Context<Self>) {
        let Some(card) = &mut self.boxd_card else {
            return;
        };
        match update {
            Update::Overview(Ok(overview)) => card.account = Account::Known(overview),
            Update::Overview(Err(error)) => {
                card.account = Account::Known(Self::boxd_missing_overview());
                card.message = Some(error.to_string());
            }
            Update::Done(result) => {
                card.message = result.err().map(|error| error.to_string());
                self.refresh_boxd(cx);
            }
        }
    }

    fn save_boxd(&mut self, cx: &mut Context<Self>) {
        let Some(card) = &mut self.boxd_card else {
            return;
        };
        let values = card.values(cx);
        let existing = self.config.boxd.clone();
        let key = card.key.read(cx).text().trim().to_owned();
        let key = (!key.is_empty()).then(|| SecretString::from(key));
        let store = Store::for_policy(existing.allow_plaintext_credentials);
        card.key.update(cx, SearchInput::clear);
        card.message = None;
        // Reloading the saved config refreshes the account line.
        self.save_native(
            move || {
                Config::save_boxd(&values, &existing)?;
                if let Some(key) = &key {
                    setup::save_key(store, Some(key))?;
                }
                Ok(())
            },
            cx,
        );
    }

    fn forget_boxd_key(&mut self, cx: &mut Context<Self>) {
        let store = Store::for_policy(self.config.boxd.allow_plaintext_credentials);
        self.boxd_job(cx, move |_, send| {
            send(Update::Done(setup::save_key(store, None)));
        });
    }

    fn forget_boxd_device(&mut self, id: String, cx: &mut Context<Self>) {
        self.boxd_job(cx, move |_, send| {
            send(Update::Done(setup::forget_device(&id)));
        });
    }

    /// Hand a new machine to the main window, which runs and reports the job
    /// so it survives this window closing.
    fn create_boxd_machine(&mut self, cx: &mut Context<Self>) {
        let Some(settings) = self.boxd_settings() else {
            return;
        };
        let Some(card) = &mut self.boxd_card else {
            return;
        };
        let name = setup::random_name();
        let request = AddRequest {
            settings,
            name: name.clone(),
            install: card.install,
        };
        let run = card.setup;
        let script = self.cloud_setup_script(CloudProvider::Boxd, run);
        let started = self.source.update(cx, |window, cx| {
            window.start_cloud_job(
                CloudProvider::Boxd,
                name.clone(),
                script,
                move |cancelled, report| setup::add_device(request, cancelled, report),
                cx,
            )
        });
        if let Some(card) = &mut self.boxd_card {
            card.message = Some(match started {
                Ok(Ok(())) => fl!("settings-cloud-creating", name = name),
                Ok(Err(error)) => error.to_string(),
                Err(_) => fl!("settings-cloud-boxd-no-window"),
            });
        }
        cx.notify();
    }

    pub(super) fn boxd_config_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(card) = &mut self.boxd_card {
            let loaded = loaded(&self.config);
            follow_config(&card.api_url, [&card.loaded.api_url], [&loaded.api_url], cx);
            card.loaded = loaded;
        }
        if self.boxd_card.is_some() && self.section == Section::CloudDevices {
            self.refresh_boxd(cx);
        }
    }

    pub(super) fn render_boxd_card(&self, cx: &mut Context<Self>) -> Div {
        let Some(card) = &self.boxd_card else {
            return div();
        };
        let ready = !self.busy();
        let configured = self.boxd_settings().is_some();
        let (status, key) = match &card.account {
            _ if !configured => (fl!("settings-cloud-boxd-not-configured"), Key::Missing),
            Account::Checking => (fl!("settings-cloud-checking"), Key::Missing),
            Account::Known(overview) => (
                match (&overview.key, &overview.machines) {
                    (Key::Missing, _) => fl!("settings-cloud-boxd-key-none"),
                    (_, Some(Ok(count))) => fl!("settings-cloud-boxd-count", count = *count),
                    (_, Some(Err(error))) => {
                        fl!("settings-cloud-boxd-key-unusable", error = error.localize())
                    }
                    (_, None) => fl!("settings-cloud-checking"),
                },
                overview.key,
            ),
        };
        let usable = matches!(&card.account, Account::Known(overview)
            if matches!(overview.machines, Some(Ok(_))));
        let key_note = match key {
            Key::Environment => fl!("settings-cloud-boxd-key-environment"),
            Key::Saved => fl!("settings-cloud-boxd-key-saved"),
            Key::Missing => fl!("settings-cloud-boxd-key-missing"),
        };
        let account = self
            .cloud_card(fl!(
                "settings-cloud-experimental",
                provider = crate::cloud::name(CloudProvider::Boxd)
            ))
            .debug_selector(|| "cloud-boxd-card".into())
            .child(self.control_note(fl!("settings-cloud-boxd-note")))
            .child(div().min_w_0().child(status))
            .child(self.cloud_field(
                fl!("settings-cloud-boxd-api-url"),
                card.api_url[0].clone().into(),
            ))
            .child(self.cloud_field(fl!("settings-cloud-boxd-key"), card.key.clone().into()))
            .child(
                div()
                    .row()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(12.))
                    .child(self.control_note(key_note))
                    .when(key == Key::Saved, |row| {
                        row.child(
                            self.cloud_button(
                                "cloud-boxd-forget-key",
                                fl!("settings-cloud-boxd-forget-key"),
                                ready,
                            )
                            .when(ready, |button| {
                                button.on_click(
                                    cx.listener(|this, _, _, cx| this.forget_boxd_key(cx)),
                                )
                            }),
                        )
                    }),
            )
            .child(
                div().row().justify_end().child(
                    self.cloud_button("cloud-boxd-save", fl!("settings-cloud-save"), ready)
                        .debug_selector(|| "cloud-boxd-save".into())
                        .when(ready, |button| {
                            button.on_click(cx.listener(|this, _, _, cx| this.save_boxd(cx)))
                        }),
                ),
            );
        let create = ready && usable && crate::cloud::unavailable().is_none();
        let mut new = self
            .cloud_card(fl!("settings-cloud-boxd-new"))
            .debug_selector(|| "cloud-boxd-new".into())
            .child(self.control_note(fl!("settings-cloud-boxd-new-note")))
            .child(
                self.control_switch(
                    "cloud-boxd-install",
                    fl!("settings-cloud-install-herdr-in-it"),
                    card.install,
                    ready,
                )
                .when(ready, |row| {
                    row.on_click(cx.listener(|this, _, _, cx| {
                        if let Some(card) = &mut this.boxd_card {
                            card.install = !card.install;
                        }
                        cx.notify();
                    }))
                }),
            )
            .children(self.cloud_setup_switch(
                CloudProvider::Boxd,
                card.setup,
                ready,
                |this| {
                    if let Some(card) = &mut this.boxd_card {
                        card.setup = !card.setup;
                    }
                },
                cx,
            ))
            .child(
                div().row().justify_end().child(
                    self.cloud_button(
                        "cloud-boxd-create",
                        fl!("settings-cloud-boxd-create"),
                        create,
                    )
                    .debug_selector(|| "cloud-boxd-create".into())
                    .when(create, |button| {
                        button.on_click(cx.listener(|this, _, _, cx| this.create_boxd_machine(cx)))
                    }),
                ),
            );
        if let Some(message) = &card.message {
            new = new.child(div().min_w_0().child(message.clone()));
        }
        let mut devices = self
            .cloud_card(fl!("settings-cloud-boxd-devices"))
            .debug_selector(|| "cloud-boxd-devices".into())
            .child(self.control_note(fl!("settings-cloud-boxd-devices-note")));
        if card.devices().is_empty() {
            devices = devices.child(self.control_note(fl!("settings-cloud-none-yet")));
        }
        for (index, device) in card.devices().iter().enumerate() {
            devices = devices.child(self.cloud_device_row(
                index,
                device,
                format!("{} · {}", device.machine, device.session),
                ready,
                Self::forget_boxd_device,
                cx,
            ));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .min_w_0()
            .child(account)
            .child(new)
            .child(devices)
    }
}

#[cfg(test)]
mod tests;
