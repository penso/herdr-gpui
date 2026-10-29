//! The Sprites tab of Settings > Cloud devices. Its API URL saves to
//! `[sprites]` in the local config; its token saves to the credential store.
//! Creating a sprite hands a job to the main window, which shows progress in
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
    config::{Config, SpritesFields},
    github::Store,
    i18n::fl,
    search_input::SearchInput,
    sprites::{
        Settings,
        setup::{self, AddRequest, Overview, Token},
    },
};
use gpui::{prelude::*, *};
use secrecy::SecretString;

enum Update {
    Overview(crate::sprites::Result<Overview>),
    Done(crate::sprites::Result<()>),
}

/// The account line, while a job may be changing it.
enum Account {
    Checking,
    Known(Overview),
}

pub(in crate::settings_window) struct SpritesCard {
    api_url: [Entity<SearchInput>; 1],
    /// The config values the field last loaded; see `follow_config`.
    loaded: SpritesFields,
    token: Entity<SearchInput>,
    account: Account,
    /// The user's approval to install Herdr in sprites created from here.
    install: bool,
    /// Run the setup script on the next device made here, when one is set.
    setup: bool,
    message: Option<String>,
    job: Option<Worker>,
}

/// The values the field shows for `config`. Fly's own service is the usual
/// account, so an unset API URL shows it; saving it sets Sprites up.
fn loaded(config: &Config) -> SpritesFields {
    let mut values = SpritesFields::from_config(&config.sprites);
    if values.api_url.is_empty() {
        values.api_url = crate::sprites::DEFAULT_API_URL.into();
    }
    values
}

impl SpritesCard {
    fn new(
        config: &Config,
        theme: &crate::config::Theme,
        cx: &mut Context<SettingsWindow>,
    ) -> Self {
        let loaded = loaded(config);
        let api_url = [input(
            || crate::sprites::DEFAULT_API_URL.to_owned(),
            &loaded.api_url,
            config,
            theme,
            cx,
        )];
        let token = input(
            || fl!("settings-cloud-sprites-token-placeholder"),
            "",
            config,
            theme,
            cx,
        );
        token.update(cx, |input, cx| input.set_masked(true, cx));
        Self {
            api_url,
            loaded,
            token,
            account: Account::Checking,
            install: true,
            setup: true,
            message: None,
            job: None,
        }
    }

    fn values(&self, cx: &App) -> SpritesFields {
        SpritesFields {
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
    pub(super) fn open_sprites_card(&mut self, cx: &mut Context<Self>) {
        if self.sprites_card.is_none() {
            self.sprites_card = Some(SpritesCard::new(&self.config, &self.theme, cx));
        }
        self.refresh_sprites(cx);
    }

    fn sprites_settings(&self) -> Option<Settings> {
        self.config.sprites.settings().ok().flatten()
    }

    fn sprites_job(
        &mut self,
        cx: &mut Context<Self>,
        work: impl FnOnce(&dyn Fn() -> bool, &dyn Fn(Update)) + Send + 'static,
    ) {
        let Some(card) = &mut self.sprites_card else {
            return;
        };
        card.job = None;
        match worker::spawn(
            "herdr-sprites-settings",
            cx,
            work,
            |this: &mut Self, update, cx| this.apply_sprites(update, cx),
        ) {
            Ok(job) => {
                if let Some(card) = &mut self.sprites_card {
                    card.job = Some(job);
                }
            }
            Err(error) => {
                tracing::error!(category = "sprites_worker", error_kind = ?error.kind(), "Could not start the Sprites settings worker");
                if let Some(card) = &mut self.sprites_card {
                    // i18n: data
                    card.message = Some(crate::cloud::Error::Worker("settings").to_string());
                }
            }
        }
    }

    fn missing_overview() -> Overview {
        Overview {
            token: Token::Missing,
            sprites: None,
            devices: Vec::new(),
        }
    }

    fn refresh_sprites(&mut self, cx: &mut Context<Self>) {
        let Some(settings) = self.sprites_settings() else {
            if let Some(card) = &mut self.sprites_card {
                card.job = None;
                card.account = Account::Known(Self::missing_overview());
            }
            return;
        };
        if let Some(card) = &mut self.sprites_card {
            card.account = Account::Checking;
        }
        self.sprites_job(cx, move |_, send| {
            send(Update::Overview(setup::overview(&settings)));
        });
    }

    fn apply_sprites(&mut self, update: Update, cx: &mut Context<Self>) {
        let Some(card) = &mut self.sprites_card else {
            return;
        };
        match update {
            Update::Overview(Ok(overview)) => card.account = Account::Known(overview),
            Update::Overview(Err(error)) => {
                card.account = Account::Known(Self::missing_overview());
                card.message = Some(error.to_string());
            }
            Update::Done(result) => {
                card.message = result.err().map(|error| error.to_string());
                self.refresh_sprites(cx);
            }
        }
    }

    fn save_sprites(&mut self, cx: &mut Context<Self>) {
        let Some(card) = &mut self.sprites_card else {
            return;
        };
        let values = card.values(cx);
        let existing = self.config.sprites.clone();
        let token = card.token.read(cx).text().trim().to_owned();
        let token = (!token.is_empty()).then(|| SecretString::from(token));
        let store = Store::for_policy(existing.allow_plaintext_credentials);
        card.token.update(cx, SearchInput::clear);
        card.message = None;
        // Reloading the saved config refreshes the account line.
        self.save_native(
            move || {
                Config::save_sprites(&values, &existing)?;
                if let Some(token) = &token {
                    setup::save_token(store, Some(token))?;
                }
                Ok(())
            },
            cx,
        );
    }

    fn forget_sprites_token(&mut self, cx: &mut Context<Self>) {
        let store = Store::for_policy(self.config.sprites.allow_plaintext_credentials);
        self.sprites_job(cx, move |_, send| {
            send(Update::Done(setup::save_token(store, None)));
        });
    }

    fn forget_sprites_device(&mut self, id: String, cx: &mut Context<Self>) {
        self.sprites_job(cx, move |_, send| {
            send(Update::Done(setup::forget_device(&id)));
        });
    }

    /// Hand a new sprite to the main window, which runs and reports the job
    /// so it survives this window closing.
    fn create_sprite(&mut self, cx: &mut Context<Self>) {
        let Some(settings) = self.sprites_settings() else {
            return;
        };
        let Some(card) = &mut self.sprites_card else {
            return;
        };
        let name = setup::random_name();
        let request = AddRequest {
            settings,
            name: name.clone(),
            install: card.install,
        };
        let run = card.setup;
        let script = self.cloud_setup_script(CloudProvider::Sprites, run);
        let started = self.source.update(cx, |window, cx| {
            window.start_cloud_job(
                CloudProvider::Sprites,
                name.clone(),
                script,
                move |cancelled, report| setup::add_device(request, cancelled, report),
                cx,
            )
        });
        if let Some(card) = &mut self.sprites_card {
            card.message = Some(match started {
                Ok(Ok(())) => fl!("settings-cloud-creating", name = name),
                Ok(Err(error)) => error.to_string(),
                Err(_) => fl!("settings-cloud-sprites-no-window"),
            });
        }
        cx.notify();
    }

    pub(super) fn sprites_config_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(card) = &mut self.sprites_card {
            let loaded = loaded(&self.config);
            follow_config(&card.api_url, [&card.loaded.api_url], [&loaded.api_url], cx);
            card.loaded = loaded;
        }
        if self.sprites_card.is_some() && self.section == Section::CloudDevices {
            self.refresh_sprites(cx);
        }
    }

    pub(super) fn render_sprites_card(&self, cx: &mut Context<Self>) -> Div {
        let Some(card) = &self.sprites_card else {
            return div();
        };
        let ready = !self.busy();
        let configured = self.sprites_settings().is_some();
        let (status, token) = match &card.account {
            _ if !configured => (fl!("settings-cloud-sprites-not-configured"), Token::Missing),
            Account::Checking => (fl!("settings-cloud-checking"), Token::Missing),
            Account::Known(overview) => (
                match (&overview.token, &overview.sprites) {
                    (Token::Missing, _) => fl!("settings-cloud-sprites-token-none"),
                    (_, Some(Ok(count))) => fl!("settings-cloud-sprites-count", count = *count),
                    (_, Some(Err(error))) => fl!(
                        "settings-cloud-sprites-token-unusable",
                        error = error.to_string()
                    ),
                    (_, None) => fl!("settings-cloud-checking"),
                },
                overview.token,
            ),
        };
        let usable = matches!(&card.account, Account::Known(overview)
            if matches!(overview.sprites, Some(Ok(_))));
        let token_note = match token {
            Token::Environment => fl!("settings-cloud-sprites-token-environment"),
            Token::Saved => fl!("settings-cloud-sprites-token-saved"),
            Token::Missing => fl!("settings-cloud-sprites-token-missing"),
        };
        let account = self
            .cloud_card(fl!(
                "settings-cloud-experimental",
                provider = crate::cloud::name(CloudProvider::Sprites)
            ))
            .debug_selector(|| "cloud-sprites-card".into())
            .child(self.control_note(fl!("settings-cloud-sprites-note")))
            .child(div().min_w_0().child(status))
            .child(self.cloud_field(
                fl!("settings-cloud-sprites-api-url"),
                card.api_url[0].clone().into(),
            ))
            .child(self.cloud_field(
                fl!("settings-cloud-sprites-token"),
                card.token.clone().into(),
            ))
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
                                "cloud-sprites-forget-token",
                                fl!("settings-cloud-sprites-forget-token"),
                                ready,
                            )
                            .when(ready, |button| {
                                button.on_click(
                                    cx.listener(|this, _, _, cx| this.forget_sprites_token(cx)),
                                )
                            }),
                        )
                    }),
            )
            .child(
                div().flex().justify_end().child(
                    self.cloud_button("cloud-sprites-save", fl!("settings-cloud-save"), ready)
                        .debug_selector(|| "cloud-sprites-save".into())
                        .when(ready, |button| {
                            button.on_click(cx.listener(|this, _, _, cx| this.save_sprites(cx)))
                        }),
                ),
            );
        let create = ready && usable && crate::cloud::unavailable().is_none();
        let mut new = self
            .cloud_card(fl!("settings-cloud-sprites-new"))
            .debug_selector(|| "cloud-sprites-new".into())
            .child(self.control_note(fl!("settings-cloud-sprites-new-note")))
            .child(
                self.control_switch(
                    "cloud-sprites-install",
                    fl!("settings-cloud-install-herdr-in-it"),
                    card.install,
                    ready,
                )
                .when(ready, |row| {
                    row.on_click(cx.listener(|this, _, _, cx| {
                        if let Some(card) = &mut this.sprites_card {
                            card.install = !card.install;
                        }
                        cx.notify();
                    }))
                }),
            )
            .children(self.cloud_setup_switch(
                CloudProvider::Sprites,
                card.setup,
                ready,
                |this| {
                    if let Some(card) = &mut this.sprites_card {
                        card.setup = !card.setup;
                    }
                },
                cx,
            ))
            .child(
                div().flex().justify_end().child(
                    self.cloud_button(
                        "cloud-sprites-create",
                        fl!("settings-cloud-sprites-create"),
                        create,
                    )
                    .debug_selector(|| "cloud-sprites-create".into())
                    .when(create, |button| {
                        button.on_click(cx.listener(|this, _, _, cx| this.create_sprite(cx)))
                    }),
                ),
            );
        if let Some(message) = &card.message {
            new = new.child(div().min_w_0().child(message.clone()));
        }
        let mut devices = self
            .cloud_card(fl!("settings-cloud-sprites-devices"))
            .debug_selector(|| "cloud-sprites-devices".into())
            .child(self.control_note(fl!("settings-cloud-sprites-devices-note")));
        if card.devices().is_empty() {
            devices = devices.child(self.control_note(fl!("settings-cloud-none-yet")));
        }
        for (index, device) in card.devices().iter().enumerate() {
            devices = devices.child(self.cloud_device_row(
                index,
                device,
                format!("{} · {}", device.machine, device.session),
                ready,
                Self::forget_sprites_device,
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
