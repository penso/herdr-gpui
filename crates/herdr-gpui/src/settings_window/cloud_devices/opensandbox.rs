//! The OpenSandbox tab of Settings > Cloud devices. Its server and sandbox
//! size save to `[opensandbox]` in the local config; its API key saves to the
//! credential store. Creating a sandbox hands a job to the main window, which shows progress in
//! the device picker and a toast when it is ready. Account reads and removals
//! run through `cloud::worker`, never on the UI thread.
// i18n: migrated

use super::{
    super::{Section, SettingsWindow},
    follow_config, input,
};
use crate::{
    cloud::{
        CloudProvider, SavedDevice,
        worker::{self, Worker},
    },
    config::{Config, OpenSandboxFields},
    github::Store,
    i18n::{self, fl},
    opensandbox::{
        Settings,
        setup::{self, AddRequest, Key as Token, Overview},
    },
    search_input::SearchInput,
};
use gpui::{prelude::*, *};
use secrecy::SecretString;

enum Update {
    Overview(crate::opensandbox::Result<Overview>),
    Done(crate::opensandbox::Result<()>),
}

/// The account line, while a job may be changing it.
enum Account {
    Checking,
    Known(Overview),
}

/// The editable `[opensandbox]` keys, in display order, with their labels'
/// message IDs and their defaults as hints.
const FIELDS: [(&str, &str); 4] = [
    (
        "settings-cloud-opensandbox-url",
        crate::opensandbox::DEFAULT_API_URL,
    ),
    (
        "settings-cloud-opensandbox-image",
        "buildpack-deps:bookworm",
    ),
    ("settings-cloud-opensandbox-cpu", "2"),
    ("settings-cloud-opensandbox-memory", "4Gi"),
];

pub(in crate::settings_window) struct OpenSandboxCard {
    fields: [Entity<SearchInput>; 4],
    /// The config values the field last loaded; see `follow_config`.
    loaded: OpenSandboxFields,
    token: Entity<SearchInput>,
    account: Account,
    /// The user's approval to install Herdr in sandboxes created from here.
    install: bool,
    /// Run the setup script on the next device made here, when one is set.
    setup: bool,
    message: Option<String>,
    job: Option<Worker>,
}

/// The values the fields show for `config`. A server on this computer is the
/// usual start, so an unset URL shows it; saving it sets OpenSandbox up.
fn loaded(config: &Config) -> OpenSandboxFields {
    let mut values = OpenSandboxFields::from_config(&config.opensandbox);
    if values.api_url.is_empty() {
        values.api_url = crate::opensandbox::DEFAULT_API_URL.into();
    }
    values
}

/// The editable values in `FIELDS` order.
fn texts(values: &OpenSandboxFields) -> [&str; 4] {
    [&values.api_url, &values.image, &values.cpu, &values.memory]
}

impl OpenSandboxCard {
    fn new(
        config: &Config,
        theme: &crate::config::Theme,
        cx: &mut Context<SettingsWindow>,
    ) -> Self {
        let loaded = loaded(config);
        let texts = texts(&loaded);
        let fields = std::array::from_fn(|index| {
            let placeholder = FIELDS[index].1;
            input(
                move || placeholder.to_owned(),
                texts[index],
                config,
                theme,
                cx,
            )
        });
        let token = input(
            || fl!("settings-cloud-opensandbox-key-placeholder"),
            "",
            config,
            theme,
            cx,
        );
        token.update(cx, |input, cx| input.set_masked(true, cx));
        Self {
            fields,
            loaded,
            token,
            account: Account::Checking,
            install: true,
            setup: true,
            message: None,
            job: None,
        }
    }

    fn values(&self, cx: &App) -> OpenSandboxFields {
        let text = |index: usize| self.fields[index].read(cx).text().trim().to_owned();
        OpenSandboxFields {
            api_url: text(0),
            image: text(1),
            cpu: text(2),
            memory: text(3),
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
    pub(super) fn open_opensandbox_card(&mut self, cx: &mut Context<Self>) {
        if self.opensandbox_card.is_none() {
            self.opensandbox_card = Some(OpenSandboxCard::new(&self.config, &self.theme, cx));
        }
        self.refresh_opensandbox(cx);
    }

    fn opensandbox_settings(&self) -> Option<Settings> {
        self.config.opensandbox.settings().ok().flatten()
    }

    fn opensandbox_job(
        &mut self,
        cx: &mut Context<Self>,
        work: impl FnOnce(&dyn Fn() -> bool, &dyn Fn(Update)) + Send + 'static,
    ) {
        let Some(card) = &mut self.opensandbox_card else {
            return;
        };
        card.job = None;
        match worker::spawn(
            "herdr-opensandbox-settings",
            cx,
            work,
            |this: &mut Self, update, cx| this.apply_opensandbox(update, cx),
        ) {
            Ok(job) => {
                if let Some(card) = &mut self.opensandbox_card {
                    card.job = Some(job);
                }
            }
            Err(error) => {
                tracing::error!(category = "opensandbox_worker", error_kind = ?error.kind(), "Could not start the OpenSandbox settings worker");
                if let Some(card) = &mut self.opensandbox_card {
                    // i18n: data
                    card.message = Some(crate::cloud::Error::Worker("settings").to_string());
                }
            }
        }
    }

    fn missing_opensandbox_overview() -> Overview {
        Overview {
            key: Token::Missing,
            sandboxes: None,
            devices: Vec::new(),
        }
    }

    fn refresh_opensandbox(&mut self, cx: &mut Context<Self>) {
        let Some(settings) = self.opensandbox_settings() else {
            if let Some(card) = &mut self.opensandbox_card {
                card.job = None;
                card.account = Account::Known(Self::missing_opensandbox_overview());
            }
            return;
        };
        if let Some(card) = &mut self.opensandbox_card {
            card.account = Account::Checking;
        }
        self.opensandbox_job(cx, move |_, send| {
            send(Update::Overview(setup::overview(&settings)));
        });
    }

    fn apply_opensandbox(&mut self, update: Update, cx: &mut Context<Self>) {
        let Some(card) = &mut self.opensandbox_card else {
            return;
        };
        match update {
            Update::Overview(Ok(overview)) => card.account = Account::Known(overview),
            Update::Overview(Err(error)) => {
                card.account = Account::Known(Self::missing_opensandbox_overview());
                card.message = Some(error.to_string());
            }
            Update::Done(result) => {
                card.message = result.err().map(|error| error.to_string());
                self.refresh_opensandbox(cx);
            }
        }
    }

    fn save_opensandbox(&mut self, cx: &mut Context<Self>) {
        let Some(card) = &mut self.opensandbox_card else {
            return;
        };
        let values = card.values(cx);
        let existing = self.config.opensandbox.clone();
        let token = card.token.read(cx).text().trim().to_owned();
        let token = (!token.is_empty()).then(|| SecretString::from(token));
        let store = Store::for_policy(existing.allow_plaintext_credentials);
        card.token.update(cx, SearchInput::clear);
        card.message = None;
        // Reloading the saved config refreshes the account line.
        self.save_native(
            move || {
                Config::save_opensandbox(&values, &existing)?;
                if let Some(token) = &token {
                    setup::save_key(store, Some(token))?;
                }
                Ok(())
            },
            cx,
        );
    }

    fn forget_opensandbox_key(&mut self, cx: &mut Context<Self>) {
        let store = Store::for_policy(self.config.opensandbox.allow_plaintext_credentials);
        self.opensandbox_job(cx, move |_, send| {
            send(Update::Done(setup::save_key(store, None)));
        });
    }

    fn forget_opensandbox_device(&mut self, id: String, cx: &mut Context<Self>) {
        self.opensandbox_job(cx, move |_, send| {
            send(Update::Done(setup::forget_device(&id)));
        });
    }

    /// Hand a new sandbox to the main window, which runs and reports the job
    /// so it survives this window closing.
    fn create_opensandbox(&mut self, cx: &mut Context<Self>) {
        let Some(settings) = self.opensandbox_settings() else {
            return;
        };
        let Some(card) = &mut self.opensandbox_card else {
            return;
        };
        let name = setup::random_name();
        let request = AddRequest {
            settings,
            name: name.clone(),
            install: card.install,
        };
        let run = card.setup;
        let script = self.cloud_setup_script(CloudProvider::OpenSandbox, run);
        let started = self.source.update(cx, |window, cx| {
            window.start_cloud_job(
                CloudProvider::OpenSandbox,
                name.clone(),
                script,
                move |cancelled, report| setup::add_device(request, cancelled, report),
                cx,
            )
        });
        if let Some(card) = &mut self.opensandbox_card {
            card.message = Some(match started {
                Ok(Ok(())) => fl!("settings-cloud-creating", name = name),
                Ok(Err(error)) => error.to_string(),
                Err(_) => fl!("settings-cloud-no-window-sandbox"),
            });
        }
        cx.notify();
    }

    pub(super) fn opensandbox_config_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(card) = &mut self.opensandbox_card {
            let loaded = loaded(&self.config);
            follow_config(&card.fields, texts(&card.loaded), texts(&loaded), cx);
            card.loaded = loaded;
        }
        if self.opensandbox_card.is_some() && self.section == Section::CloudDevices {
            self.refresh_opensandbox(cx);
        }
    }

    pub(super) fn render_opensandbox_card(&self, cx: &mut Context<Self>) -> Div {
        let Some(card) = &self.opensandbox_card else {
            return div();
        };
        let ready = !self.busy();
        let configured = self.opensandbox_settings().is_some();
        let (status, token) = match &card.account {
            _ if !configured => (
                fl!("settings-cloud-opensandbox-not-configured"),
                Token::Missing,
            ),
            Account::Checking => (fl!("settings-cloud-checking"), Token::Missing),
            Account::Known(overview) => (
                match (&overview.key, &overview.sandboxes) {
                    (Token::Missing, _) => fl!("settings-cloud-key-missing"),
                    (_, Some(Ok(count))) => {
                        fl!("settings-cloud-opensandbox-sandboxes", count = *count)
                    }
                    (_, Some(Err(error))) => fl!(
                        "settings-cloud-opensandbox-unusable",
                        error = error.to_string()
                    ),
                    (_, None) => fl!("settings-cloud-checking"),
                },
                overview.key,
            ),
        };
        let usable = matches!(&card.account, Account::Known(overview)
            if matches!(overview.sandboxes, Some(Ok(_))));
        let token_note = match token {
            Token::Environment => fl!("settings-cloud-opensandbox-key-environment"),
            Token::Saved => fl!("settings-cloud-key-saved"),
            Token::Missing => fl!("settings-cloud-opensandbox-key-missing"),
        };
        let account = self
            .cloud_card(fl!(
                "settings-cloud-experimental",
                provider = crate::cloud::name(CloudProvider::OpenSandbox)
            ))
            .debug_selector(|| "cloud-opensandbox-card".into())
            .child(self.control_note(fl!("settings-cloud-opensandbox-note")))
            .child(div().min_w_0().child(status))
            .child(self.cloud_field(
                i18n::message(FIELDS[0].0, &[]),
                card.fields[0].clone().into(),
            ))
            .child(self.cloud_field(fl!("settings-cloud-api-key"), card.token.clone().into()))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap(px(12.))
                    .child(self.control_note(token_note))
                    .when(token == Token::Saved, |row| {
                        row.child(
                            self.cloud_button(
                                "cloud-opensandbox-forget-key",
                                fl!("settings-cloud-forget-key"),
                                ready,
                            )
                            .when(ready, |button| {
                                button.on_click(
                                    cx.listener(|this, _, _, cx| this.forget_opensandbox_key(cx)),
                                )
                            }),
                        )
                    }),
            )
            .children((1..FIELDS.len()).map(|index| {
                self.cloud_field(
                    i18n::message(FIELDS[index].0, &[]),
                    card.fields[index].clone().into(),
                )
            }))
            .child(
                div().flex().justify_end().child(
                    self.cloud_button("cloud-opensandbox-save", fl!("settings-cloud-save"), ready)
                        .debug_selector(|| "cloud-opensandbox-save".into())
                        .when(ready, |button| {
                            button.on_click(cx.listener(|this, _, _, cx| this.save_opensandbox(cx)))
                        }),
                ),
            );
        let create = ready && usable && crate::cloud::unavailable().is_none();
        let mut new = self
            .cloud_card(fl!("settings-cloud-new-sandbox"))
            .debug_selector(|| "cloud-opensandbox-new".into())
            .child(self.control_note(fl!("settings-cloud-opensandbox-new-note")))
            .child(
                self.control_switch(
                    "cloud-opensandbox-install",
                    fl!("settings-cloud-install-herdr-in-it"),
                    card.install,
                    ready,
                )
                .when(ready, |row| {
                    row.on_click(cx.listener(|this, _, _, cx| {
                        if let Some(card) = &mut this.opensandbox_card {
                            card.install = !card.install;
                        }
                        cx.notify();
                    }))
                }),
            )
            .children(self.cloud_setup_switch(
                CloudProvider::OpenSandbox,
                card.setup,
                ready,
                |this| {
                    if let Some(card) = &mut this.opensandbox_card {
                        card.setup = !card.setup;
                    }
                },
                cx,
            ))
            .child(
                div().flex().justify_end().child(
                    self.cloud_button(
                        "cloud-opensandbox-create",
                        fl!("settings-cloud-create-sandbox"),
                        create,
                    )
                    .debug_selector(|| "cloud-opensandbox-create".into())
                    .when(create, |button| {
                        button.on_click(cx.listener(|this, _, _, cx| this.create_opensandbox(cx)))
                    }),
                ),
            );
        if let Some(message) = &card.message {
            new = new.child(div().min_w_0().child(message.clone()));
        }
        let mut devices = self
            .cloud_card(fl!("settings-cloud-opensandbox-devices"))
            .debug_selector(|| "cloud-opensandbox-devices".into())
            .child(self.control_note(fl!("settings-cloud-opensandbox-devices-note")));
        if card.devices().is_empty() {
            devices = devices.child(self.control_note(fl!("settings-cloud-none-yet")));
        }
        for (index, device) in card.devices().iter().enumerate() {
            devices = devices.child(self.cloud_device_row(
                index,
                device,
                format!("{} · {}", device.machine, device.session),
                ready,
                Self::forget_opensandbox_device,
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
