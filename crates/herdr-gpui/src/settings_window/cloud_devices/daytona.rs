//! The Daytona tab of Settings > Cloud devices. Its account fields save to
//! `[daytona]` in the local config; its API key saves to the credential store.
//! Creating a sandbox hands a job to the main window, which shows progress in
//! the device picker and a toast when it is ready. Account reads and removals
//! run through `cloud::worker`, never on the UI thread.
// i18n: migrated

use super::{
    super::{Section, SettingsWindow},
    Hint, follow_config, input,
};
use crate::{
    cloud::{
        CloudProvider, SavedDevice,
        worker::{self, Worker},
    },
    config::{Config, DaytonaFields},
    daytona::{
        Settings,
        setup::{self, AddRequest, Key, Overview},
    },
    github::Store,
    i18n::{self, fl},
    search_input::SearchInput,
};
use gpui::{prelude::*, *};
use secrecy::SecretString;

/// The editable `[daytona]` keys, in display order, with their labels'
/// message IDs and hints.
const FIELDS: [(&str, Hint); 4] = [
    (
        "settings-cloud-daytona-api-url",
        Hint::Value(crate::daytona::DEFAULT_API_URL),
    ),
    (
        "settings-cloud-daytona-organization",
        Hint::Message("settings-cloud-daytona-organization-placeholder"),
    ),
    (
        "settings-cloud-daytona-region",
        Hint::Message("settings-cloud-daytona-region-placeholder"),
    ),
    (
        "settings-cloud-daytona-snapshot",
        Hint::Message("settings-cloud-daytona-snapshot-placeholder"),
    ),
];

enum Update {
    Overview(crate::daytona::Result<Overview>),
    Done(crate::daytona::Result<()>),
}

/// The account line, while a job may be changing it.
enum Account {
    Checking,
    Known(Overview),
}

pub(in crate::settings_window) struct DaytonaCard {
    fields: [Entity<SearchInput>; 4],
    /// The config values the fields last loaded; see `follow_config`.
    loaded: DaytonaFields,
    key: Entity<SearchInput>,
    account: Account,
    /// The user's approval to install Herdr in sandboxes created from here.
    install: bool,
    /// Run the setup script on the next device made here, when one is set.
    setup: bool,
    message: Option<String>,
    job: Option<Worker>,
}

impl DaytonaCard {
    fn new(
        config: &Config,
        theme: &crate::config::Theme,
        cx: &mut Context<SettingsWindow>,
    ) -> Self {
        let loaded = loaded(config);
        let texts = texts(&loaded);
        let fields = std::array::from_fn(|index| {
            let hint = FIELDS[index].1;
            input(move || hint.text(), texts[index], config, theme, cx)
        });
        // A Daytona key's prefix.
        let key = input(|| "dtn_…".to_owned(), "", config, theme, cx); // i18n: data
        key.update(cx, |input, cx| input.set_masked(true, cx));
        Self {
            fields,
            loaded,
            key,
            account: Account::Checking,
            install: true,
            setup: true,
            message: None,
            job: None,
        }
    }

    fn values(&self, cx: &App) -> DaytonaFields {
        let text = |index: usize| self.fields[index].read(cx).text().trim().to_owned();
        DaytonaFields {
            api_url: text(0),
            organization_id: text(1),
            target: text(2),
            snapshot: text(3),
        }
    }

    fn devices(&self) -> &[SavedDevice] {
        match &self.account {
            Account::Known(overview) => &overview.devices,
            Account::Checking => &[],
        }
    }
}

/// The values the fields show for `config`. Daytona's own cloud is the usual
/// account, so an unset API URL shows it; saving it sets Daytona up.
fn loaded(config: &Config) -> DaytonaFields {
    let mut values = DaytonaFields::from_config(&config.daytona);
    if values.api_url.is_empty() {
        values.api_url = crate::daytona::DEFAULT_API_URL.into();
    }
    values
}

/// The editable values in `FIELDS` order.
fn texts(values: &DaytonaFields) -> [&str; 4] {
    [
        &values.api_url,
        &values.organization_id,
        &values.target,
        &values.snapshot,
    ]
}

impl SettingsWindow {
    pub(super) fn open_daytona_card(&mut self, cx: &mut Context<Self>) {
        if self.daytona_card.is_none() {
            self.daytona_card = Some(DaytonaCard::new(&self.config, &self.theme, cx));
        }
        self.refresh_daytona(cx);
    }

    fn daytona_settings(&self) -> Option<Settings> {
        self.config.daytona.settings().ok().flatten()
    }

    fn daytona_job(
        &mut self,
        cx: &mut Context<Self>,
        work: impl FnOnce(&dyn Fn() -> bool, &dyn Fn(Update)) + Send + 'static,
    ) {
        let Some(card) = &mut self.daytona_card else {
            return;
        };
        card.job = None;
        match worker::spawn(
            "herdr-daytona-settings",
            cx,
            work,
            |this: &mut Self, update, cx| this.apply_daytona(update, cx),
        ) {
            Ok(job) => {
                if let Some(card) = &mut self.daytona_card {
                    card.job = Some(job);
                }
            }
            Err(error) => {
                tracing::error!(category = "daytona_worker", error_kind = ?error.kind(), "Could not start Daytona settings worker");
                if let Some(card) = &mut self.daytona_card {
                    // i18n: data
                    card.message = Some(crate::cloud::Error::Worker("settings").to_string());
                }
            }
        }
    }

    fn refresh_daytona(&mut self, cx: &mut Context<Self>) {
        self.watch_sandbox_images(cx);
        let Some(settings) = self.daytona_settings() else {
            if let Some(card) = &mut self.daytona_card {
                card.job = None;
                card.account = Account::Known(Overview {
                    key: Key::Missing,
                    sandboxes: None,
                    devices: Vec::new(),
                });
            }
            return;
        };
        if let Some(card) = &mut self.daytona_card {
            card.account = Account::Checking;
        }
        self.daytona_job(cx, move |_, send| {
            send(Update::Overview(setup::overview(&settings)));
        });
    }

    fn apply_daytona(&mut self, update: Update, cx: &mut Context<Self>) {
        let Some(card) = &mut self.daytona_card else {
            return;
        };
        match update {
            Update::Overview(Ok(overview)) => card.account = Account::Known(overview),
            Update::Overview(Err(error)) => {
                card.account = Account::Known(Overview {
                    key: Key::Missing,
                    sandboxes: None,
                    devices: Vec::new(),
                });
                card.message = Some(error.to_string());
            }
            Update::Done(result) => {
                card.message = result.err().map(|error| error.to_string());
                self.refresh_daytona(cx);
            }
        }
    }

    fn save_daytona(&mut self, cx: &mut Context<Self>) {
        let Some(card) = &mut self.daytona_card else {
            return;
        };
        let values = card.values(cx);
        let existing = self.config.daytona.clone();
        let key = card.key.read(cx).text().trim().to_owned();
        let key = (!key.is_empty()).then(|| SecretString::from(key));
        let store = Store::for_policy(existing.allow_plaintext_credentials);
        card.key.update(cx, SearchInput::clear);
        card.message = None;
        // Reloading the saved config refreshes the account line.
        self.save_native(
            move || {
                Config::save_daytona(&values, &existing)?;
                if let Some(key) = &key {
                    setup::save_key(store, Some(key))?;
                }
                Ok(())
            },
            cx,
        );
    }

    fn forget_daytona_key(&mut self, cx: &mut Context<Self>) {
        let store = Store::for_policy(self.config.daytona.allow_plaintext_credentials);
        self.daytona_job(cx, move |_, send| {
            send(Update::Done(setup::save_key(store, None)));
        });
    }

    fn forget_daytona_device(&mut self, id: String, cx: &mut Context<Self>) {
        self.daytona_job(cx, move |_, send| {
            send(Update::Done(setup::forget_device(&id)));
        });
    }

    /// Hand a new sandbox to the main window, which runs and reports the job
    /// so it survives this window closing.
    fn create_daytona_sandbox(&mut self, cx: &mut Context<Self>) {
        let Some(settings) = self.daytona_settings() else {
            return;
        };
        let Some(card) = &mut self.daytona_card else {
            return;
        };
        let name = setup::random_name();
        let request = AddRequest {
            settings,
            name: name.clone(),
            install: card.install,
        };
        let run = card.setup;
        let script = self.cloud_setup_script(CloudProvider::Daytona, run);
        let started = self.source.update(cx, |window, cx| {
            window.start_cloud_job(
                CloudProvider::Daytona,
                name.clone(),
                script,
                move |cancelled, report| setup::add_device(request, cancelled, report),
                cx,
            )
        });
        if let Some(card) = &mut self.daytona_card {
            card.message = Some(match started {
                Ok(Ok(())) => fl!("settings-cloud-creating", name = name),
                Ok(Err(error)) => error.to_string(),
                Err(_) => fl!("settings-cloud-no-window-sandbox"),
            });
        }
        cx.notify();
    }

    pub(super) fn daytona_config_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(card) = &mut self.daytona_card {
            let loaded = loaded(&self.config);
            follow_config(&card.fields, texts(&card.loaded), texts(&loaded), cx);
            card.loaded = loaded;
        }
        if self.daytona_card.is_some() && self.section == Section::CloudDevices {
            self.refresh_daytona(cx);
        }
    }

    pub(super) fn render_daytona_card(&self, cx: &mut Context<Self>) -> Div {
        let Some(card) = &self.daytona_card else {
            return div();
        };
        let ready = !self.busy();
        let configured = self.daytona_settings().is_some();
        let (status, key) = match &card.account {
            _ if !configured => (fl!("settings-cloud-daytona-not-configured"), Key::Missing),
            Account::Checking => (fl!("settings-cloud-checking"), Key::Missing),
            Account::Known(overview) => (
                match (&overview.key, &overview.sandboxes) {
                    (Key::Missing, _) => fl!("settings-cloud-key-missing"),
                    (_, Some(Ok(count))) => {
                        fl!("settings-cloud-daytona-sandboxes", count = *count)
                    }
                    (_, Some(Err(error))) => {
                        fl!("settings-cloud-key-unusable", error = error.to_string())
                    }
                    (_, None) => fl!("settings-cloud-checking"),
                },
                overview.key,
            ),
        };
        let usable = matches!(&card.account, Account::Known(overview)
            if matches!(overview.sandboxes, Some(Ok(_))));
        let key_note = match key {
            Key::Environment => fl!("settings-cloud-daytona-key-environment"),
            Key::Saved => fl!("settings-cloud-key-saved"),
            Key::Missing => fl!("settings-cloud-daytona-key-missing"),
        };
        let mut account = self
            .cloud_card(crate::cloud::name(CloudProvider::Daytona).to_owned())
            .debug_selector(|| "cloud-daytona-card".into())
            .child(self.control_note(fl!("settings-cloud-daytona-note")))
            .child(div().min_w_0().child(status));
        for (index, (label, _)) in FIELDS.iter().enumerate() {
            account = account.child(
                self.cloud_field(i18n::message(label, &[]), card.fields[index].clone().into()),
            );
            if index == 0 {
                account = account
                    .child(self.cloud_field(fl!("settings-cloud-api-key"), card.key.clone().into()))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_between()
                            .gap(px(12.))
                            .child(self.control_note(key_note.clone()))
                            .when(key == Key::Saved, |row| {
                                row.child(
                                    self.cloud_button(
                                        "cloud-daytona-forget-key",
                                        fl!("settings-cloud-forget-key"),
                                        ready,
                                    )
                                    .when(ready, |button| {
                                        button.on_click(
                                            cx.listener(|this, _, _, cx| {
                                                this.forget_daytona_key(cx)
                                            }),
                                        )
                                    }),
                                )
                            }),
                    );
            }
        }
        account = account.child(
            div().flex().justify_end().child(
                self.cloud_button("cloud-daytona-save", fl!("settings-cloud-save"), ready)
                    .debug_selector(|| "cloud-daytona-save".into())
                    .when(ready, |button| {
                        button.on_click(cx.listener(|this, _, _, cx| this.save_daytona(cx)))
                    }),
            ),
        );
        let create = ready && usable && crate::cloud::unavailable().is_none();
        let mut sandboxes = self
            .cloud_card(fl!("settings-cloud-new-sandbox"))
            .debug_selector(|| "cloud-daytona-new".into())
            .child(self.control_note(fl!("settings-cloud-daytona-new-note")))
            .child(
                self.control_switch(
                    "cloud-daytona-install",
                    fl!("settings-cloud-install-herdr-in-it"),
                    card.install,
                    ready,
                )
                .when(ready, |row| {
                    row.on_click(cx.listener(|this, _, _, cx| {
                        if let Some(card) = &mut this.daytona_card {
                            card.install = !card.install;
                        }
                        cx.notify();
                    }))
                }),
            )
            .children(self.cloud_setup_switch(
                CloudProvider::Daytona,
                card.setup,
                ready,
                |this| {
                    if let Some(card) = &mut this.daytona_card {
                        card.setup = !card.setup;
                    }
                },
                cx,
            ))
            .child(
                div().flex().justify_end().child(
                    self.cloud_button(
                        "cloud-daytona-create",
                        fl!("settings-cloud-create-sandbox"),
                        create,
                    )
                    .debug_selector(|| "cloud-daytona-create".into())
                    .when(create, |button| {
                        button
                            .on_click(cx.listener(|this, _, _, cx| this.create_daytona_sandbox(cx)))
                    }),
                ),
            );
        if let Some(message) = &card.message {
            sandboxes = sandboxes.child(div().min_w_0().child(message.clone()));
        }
        let mut devices = self
            .cloud_card(fl!("settings-cloud-daytona-devices"))
            .debug_selector(|| "cloud-daytona-devices".into());
        if card.devices().is_empty() {
            devices = devices.child(self.control_note(fl!("settings-cloud-none-yet")));
        }
        for (index, device) in card.devices().iter().enumerate() {
            devices = devices.child(self.cloud_device_row(
                index,
                device,
                format!("{} · {}", device.machine, device.session),
                ready,
                Self::forget_daytona_device,
                cx,
            ));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(4.))
            .min_w_0()
            .child(account)
            .child(sandboxes)
            .child(devices)
            .child(self.render_sandbox_images(CloudProvider::Daytona, cx))
    }
}

#[cfg(test)]
mod tests;
