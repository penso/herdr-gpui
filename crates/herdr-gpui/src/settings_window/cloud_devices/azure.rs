//! The Azure tab of Settings > Cloud Devices. Account fields save to `[azure]`;
//! its client secret saves to the credential store. One account offers both
//! VMs and Container Apps sandboxes, side by side. Creating either hands a job
//! to the main window, which shows progress in the device picker and a toast
//! when it is ready.
//!
//! Resolving the account may run `az`, which takes most of a second, so it
//! happens on the card's worker and render reads only the cached result.
// i18n: migrated

use super::{
    super::{Section, SettingsWindow},
    follow_config, input,
};
use crate::direction::Logical;
use crate::{
    azure::{
        Settings,
        machine::Machine,
        sandbox,
        setup::{self, AddRequest, CredentialStatus, Overview, Target},
    },
    cloud::{
        CloudProvider, SavedDevice,
        worker::{self, Worker},
    },
    config::{AzureConfig, AzureFields, Config},
    github::Store,
    i18n::{self, fl},
    search_input::SearchInput,
};
use gpui::{prelude::*, *};
use secrecy::SecretString;
use std::collections::HashMap;

mod destroy;
mod jobs;
mod sizes;

/// The VM Size field's place in [`FIELDS`]; the size choices draw it.
const VM_SIZE: usize = 3;

/// Each field's label and placeholder, as message IDs.
const FIELDS: [(&str, &str); 9] = [
    (
        "settings-cloud-azure-subscription",
        "settings-cloud-azure-subscription-placeholder",
    ),
    (
        "settings-cloud-azure-group",
        "settings-cloud-azure-group-placeholder",
    ),
    (
        "settings-cloud-azure-location",
        "settings-cloud-azure-location-placeholder",
    ),
    (
        "settings-cloud-azure-vm-size",
        "settings-cloud-azure-vm-size-placeholder",
    ),
    (
        "settings-cloud-azure-auto-stop",
        "settings-cloud-azure-auto-stop-placeholder",
    ),
    (
        "settings-cloud-azure-tenant",
        "settings-cloud-azure-tenant-placeholder",
    ),
    (
        "settings-cloud-azure-sandbox-group",
        "settings-cloud-azure-sandbox-group-placeholder",
    ),
    (
        "settings-cloud-azure-sandbox-region",
        "settings-cloud-azure-sandbox-region-placeholder",
    ),
    (
        "settings-cloud-azure-sandbox-image",
        "settings-cloud-azure-sandbox-image-placeholder",
    ),
];

enum Update {
    Account(crate::azure::Result<Account>),
    Done(crate::azure::Result<()>),
}

enum Account {
    Checking,
    Unconfigured {
        az_available: bool,
    },
    Known {
        settings: Box<Settings>,
        overview: Overview,
        /// What sizes cost in the saved regions; `None` when unreadable.
        catalog: Option<Box<crate::azure::sizes::Catalog>>,
        /// Each saved device's size, by device ID.
        sizes: HashMap<String, crate::azure::sizes::DeviceSize>,
    },
}

impl Account {
    /// Resolve the account and check its credential. Blocks: run it on a
    /// worker.
    fn load(config: &AzureConfig) -> crate::azure::Result<Self> {
        // An invalid field reads as not configured, as Save reports it.
        let Some(settings) = config.settings().ok().flatten() else {
            return Ok(Self::Unconfigured {
                az_available: crate::azure::cli::is_az_available(),
            });
        };
        let overview = setup::overview(&settings)?;
        let catalog = crate::azure::sizes::catalog(&settings)
            .inspect_err(|error| {
                tracing::warn!(category = "azure_sizes", %error, "Could not read Azure sizes");
            })
            .ok()
            .map(Box::new);
        let sizes = crate::azure::sizes::device_sizes(
            &settings,
            &overview.devices,
            catalog.as_deref().unwrap_or(&Default::default()),
        );
        Ok(Self::Known {
            settings: Box::new(settings),
            overview,
            catalog,
            sizes,
        })
    }
}

pub(in crate::settings_window) struct AzureCard {
    fields: [Entity<SearchInput>; 9],
    loaded: AzureFields,
    client_id: Entity<SearchInput>,
    secret: Entity<SearchInput>,
    account: Account,
    /// A reload of `account` is running, so its settings may be older than
    /// the saved config; Create waits rather than use them.
    refreshing: bool,
    /// The chosen sandbox vCPUs as `[azure] sandbox_cpu` saves them; empty
    /// for the default.
    sandbox_cpu: String,
    /// Whether the VM size field is open for a size the list lacks.
    vm_other: bool,
    install: bool,
    /// Run the setup script on the next machine made here, when one is set.
    setup: bool,
    message: Option<String>,
    job: Option<Worker>,
    /// The device whose Destroy is waiting for confirmation.
    confirm_destroy: Option<String>,
    destroying: Option<destroy::Destroying>,
    outcome: Option<destroy::Outcome>,
}

impl AzureCard {
    fn new(
        config: &Config,
        theme: &crate::config::Theme,
        cx: &mut Context<SettingsWindow>,
    ) -> Self {
        let loaded = loaded(config);
        let texts = texts(&loaded);
        let fields = std::array::from_fn(|index| {
            let id = FIELDS[index].1;
            input(
                move || i18n::message(id, &[]),
                texts[index],
                config,
                theme,
                cx,
            )
        });
        let client_id = input(
            || fl!("settings-cloud-azure-client-id-placeholder"),
            &loaded.client_id,
            config,
            theme,
            cx,
        );
        let secret = input(
            || fl!("settings-cloud-azure-secret-placeholder"),
            "",
            config,
            theme,
            cx,
        );
        secret.update(cx, |input, cx| input.set_masked(true, cx));

        let sandbox_cpu = loaded.sandbox_cpu.clone();
        Self {
            fields,
            loaded,
            client_id,
            secret,
            account: Account::Checking,
            refreshing: false,
            sandbox_cpu,
            vm_other: false,
            install: true,
            setup: true,
            message: None,
            job: None,
            confirm_destroy: None,
            destroying: None,
            outcome: None,
        }
    }

    fn values(&self, cx: &App) -> AzureFields {
        let text = |index: usize| self.fields[index].read(cx).text().trim().to_owned();
        AzureFields {
            subscription_id: text(0),
            resource_group: text(1),
            location: text(2),
            vm_size: text(3),
            auto_stop: text(4),
            tenant_id: text(5),
            client_id: self.client_id.read(cx).text().trim().to_owned(),
            sandbox_group: text(6),
            sandbox_location: text(7),
            sandbox_image: text(8),
            sandbox_cpu: self.sandbox_cpu.clone(),
        }
    }

    fn device_size(&self, id: &str) -> Option<String> {
        match &self.account {
            Account::Known { sizes, .. } => sizes.get(id).map(sizes::device_size),
            Account::Checking | Account::Unconfigured { .. } => None,
        }
    }

    /// The resource group `device` is in: the one it was saved in, not the
    /// group configured now.
    fn device_group<'a>(&'a self, device: &'a SavedDevice) -> &'a str {
        match &self.account {
            Account::Known { settings, .. } => settings.group_of(&device.account),
            Account::Checking | Account::Unconfigured { .. } => &device.account,
        }
    }

    fn catalog(&self) -> Option<&crate::azure::sizes::Catalog> {
        match &self.account {
            Account::Known { catalog, .. } => catalog.as_deref(),
            Account::Checking | Account::Unconfigured { .. } => None,
        }
    }

    fn devices(&self) -> &[SavedDevice] {
        match &self.account {
            Account::Known { overview, .. } => &overview.devices,
            Account::Checking | Account::Unconfigured { .. } => &[],
        }
    }
}

/// How a Save changes `[azure] use_azure_cli`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SignIn {
    Keep,
    /// Both IDs are blank and `az` is installed: Save means "use the signed-in
    /// CLI account", which resolving only does when told to.
    Cli,
    /// A client ID now names a service principal, so a CLI choice saved
    /// earlier would hide it.
    ServicePrincipal,
}

fn sign_in(account: &Account, values: &AzureFields, existing: &AzureConfig) -> SignIn {
    let blank = |text: &str| text.trim().is_empty();
    if !blank(&values.client_id) {
        return if existing.use_azure_cli == Some(true) {
            SignIn::ServicePrincipal
        } else {
            SignIn::Keep
        };
    }
    if blank(&values.subscription_id)
        && existing.use_azure_cli.is_none()
        && matches!(account, Account::Unconfigured { az_available: true })
    {
        return SignIn::Cli;
    }
    SignIn::Keep
}

fn loaded(config: &Config) -> AzureFields {
    AzureFields::from_config(&config.azure)
}

fn texts(values: &AzureFields) -> [&str; 9] {
    [
        &values.subscription_id,
        &values.resource_group,
        &values.location,
        &values.vm_size,
        &values.auto_stop,
        &values.tenant_id,
        &values.sandbox_group,
        &values.sandbox_location,
        &values.sandbox_image,
    ]
}

impl SettingsWindow {
    pub(super) fn open_azure_card(&mut self, cx: &mut Context<Self>) {
        if self.azure_card.is_none() {
            self.azure_card = Some(AzureCard::new(&self.config, &self.theme, cx));
        }
        self.refresh_azure(cx);
    }

    fn azure_settings(&self) -> Option<Settings> {
        match &self.azure_card.as_ref()?.account {
            Account::Known { settings, .. } => Some(Settings::clone(settings)),
            Account::Checking | Account::Unconfigured { .. } => None,
        }
    }

    fn azure_job(
        &mut self,
        cx: &mut Context<Self>,
        work: impl FnOnce(&dyn Fn() -> bool, &dyn Fn(Update)) + Send + 'static,
    ) {
        let Some(card) = &mut self.azure_card else {
            return;
        };
        card.job = None;
        match worker::spawn(
            "herdr-azure-settings",
            cx,
            work,
            |this: &mut Self, update, cx| match update {
                Update::Account(account) => {
                    if let Some(card) = &mut this.azure_card {
                        card.refreshing = false;
                        match account {
                            Ok(account) => card.account = account,
                            Err(error) => card.message = Some(error.to_string()),
                        }
                        let devices = card.devices();
                        let gone = card
                            .confirm_destroy
                            .as_ref()
                            .is_some_and(|id| !devices.iter().any(|device| &device.id == id));
                        if gone {
                            card.confirm_destroy = None;
                        }
                    }
                    cx.notify();
                }
                Update::Done(result) => {
                    if let Err(error) = result
                        && let Some(card) = &mut this.azure_card
                    {
                        card.message = Some(error.to_string());
                    }
                    this.refresh_azure(cx);
                }
            },
        ) {
            Ok(job) => {
                if let Some(card) = &mut self.azure_card {
                    card.job = Some(job);
                }
            }
            Err(error) => {
                tracing::error!(
                    category = "azure_worker",
                    ?error,
                    "Could not start Azure settings worker"
                );
                if let Some(card) = &mut self.azure_card {
                    card.refreshing = false;
                    // i18n: data
                    card.message = Some(crate::cloud::Error::Worker("settings").to_string());
                }
            }
        }
        cx.notify();
    }

    fn refresh_azure(&mut self, cx: &mut Context<Self>) {
        let config = self.config.azure.clone();
        if let Some(card) = &mut self.azure_card {
            card.refreshing = true;
        }
        self.azure_job(cx, move |_, send| {
            send(Update::Account(Account::load(&config)));
        });
    }

    fn save_azure(&mut self, cx: &mut Context<Self>) {
        let Some(card) = &mut self.azure_card else {
            return;
        };
        let values = card.values(cx);
        let existing = self.config.azure.clone();
        let secret = card.secret.read(cx).text().trim().to_owned();
        let secret = (!secret.is_empty()).then(|| SecretString::from(secret));
        let store = Store::for_policy(existing.allow_plaintext_credentials);
        let sign_in = sign_in(&card.account, &values, &existing);
        card.secret.update(cx, SearchInput::clear);
        card.message = None;

        self.save_native(
            move || {
                Config::save_azure(&values, &existing)?;
                match sign_in {
                    SignIn::Keep => {}
                    SignIn::Cli => {
                        Config::set_key("azure.use_azure_cli", toml_edit::Value::from(true))?
                    }
                    SignIn::ServicePrincipal => {
                        Config::unset_key("azure.use_azure_cli")?;
                    }
                }
                if let Some(secret) = &secret {
                    setup::save_secret(store, Some(secret))?;
                }
                Ok(())
            },
            cx,
        );
    }

    fn forget_azure_device(&mut self, id: String, cx: &mut Context<Self>) {
        self.azure_job(cx, move |_, send| {
            send(Update::Done(setup::forget_device(&id)));
        });
    }

    fn create_azure_vm(&mut self, cx: &mut Context<Self>) {
        let Some(settings) = self.azure_settings() else {
            return;
        };
        let Some(card) = &self.azure_card else {
            return;
        };
        let name = setup::random_name();
        let request = AddRequest {
            settings,
            target: Target::Create(name.clone()),
            install: card.install,
        };
        let script = self.cloud_setup_script(CloudProvider::Azure, card.setup);
        let started = self.source.update(cx, |window, cx| {
            window.start_cloud_job(
                CloudProvider::Azure,
                name.clone(),
                script,
                move |cancelled, report| setup::add_device(request, cancelled, report),
                cx,
            )
        });
        if let Some(card) = &mut self.azure_card {
            card.message = Some(match started {
                Ok(Ok(())) => fl!("settings-cloud-azure-creating-vm", name = name),
                Ok(Err(error)) => error.to_string(),
                Err(_) => fl!("settings-cloud-azure-no-window-vm"),
            });
        }
        cx.notify();
    }

    fn create_azure_sandbox(&mut self, cx: &mut Context<Self>) {
        let Some(settings) = self.azure_settings() else {
            return;
        };
        let Some(card) = &self.azure_card else {
            return;
        };
        let install = card.install;
        let script = self.cloud_setup_script(CloudProvider::Azure, card.setup);
        let name = setup::random_name();
        let label = name.clone();
        let started = self.source.update(cx, |window, cx| {
            window.start_cloud_job(
                CloudProvider::Azure,
                name.clone(),
                script,
                move |cancelled, report| {
                    sandbox::add_device(settings, label, install, cancelled, report)
                },
                cx,
            )
        });
        if let Some(card) = &mut self.azure_card {
            card.message = Some(match started {
                Ok(Ok(())) => fl!("settings-cloud-azure-creating-sandbox", name = name),
                Ok(Err(error)) => error.to_string(),
                Err(_) => fl!("settings-cloud-azure-no-window-sandbox"),
            });
        }
        cx.notify();
    }

    pub(super) fn azure_config_changed(&mut self, cx: &mut Context<Self>) {
        if let Some(card) = &mut self.azure_card {
            let loaded = loaded(&self.config);
            follow_config(&card.fields, texts(&card.loaded), texts(&loaded), cx);
            if card.client_id.read(cx).text().trim() == card.loaded.client_id {
                card.client_id.update(cx, |input, cx| {
                    input.set_text_selected(&loaded.client_id, cx)
                });
            }
            if card.sandbox_cpu == card.loaded.sandbox_cpu {
                card.sandbox_cpu = loaded.sandbox_cpu.clone();
            }
            card.loaded = loaded;
        }
        if self.azure_card.is_some() && self.section == Section::CloudDevices {
            self.refresh_azure(cx);
        }
    }

    /// One setting shown in both New boxes: whether a new machine without
    /// Herdr gets it installed.
    fn azure_install_switch(
        &self,
        id: &'static str,
        install: bool,
        ready: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        self.control_switch(id, fl!("settings-cloud-install-herdr"), install, ready)
            .when(ready, |row| {
                row.on_click(cx.listener(|this, _, _, cx| {
                    if let Some(card) = &mut this.azure_card {
                        card.install = !card.install;
                    }
                    cx.notify();
                }))
            })
    }

    /// The setup script switch, also shown in both New boxes. `id` keeps the
    /// two copies' click state apart.
    fn azure_setup_switch(
        &self,
        id: &'static str,
        on: bool,
        ready: bool,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let switch = self.cloud_setup_switch(
            CloudProvider::Azure,
            on,
            ready,
            |this| {
                if let Some(card) = &mut this.azure_card {
                    card.setup = !card.setup;
                }
            },
            cx,
        )?;
        Some(div().id(id).child(switch))
    }

    pub(super) fn render_azure_card(&self, cx: &mut Context<Self>) -> Div {
        let Some(card) = &self.azure_card else {
            return div();
        };
        let ready = !self.busy();
        let configured = matches!(card.account, Account::Known { .. });

        let status = match &card.account {
            Account::Unconfigured { az_available } => {
                if *az_available {
                    fl!("settings-cloud-azure-cli-detected")
                } else {
                    fl!("settings-cloud-azure-not-configured")
                }
            }
            Account::Checking => fl!("settings-cloud-checking"),
            Account::Known { overview, .. } => match &overview.credential {
                Ok(CredentialStatus::AzureCli) => fl!("settings-cloud-azure-signed-in-cli"),
                Ok(CredentialStatus::SavedSecret) => fl!("settings-cloud-azure-signed-in-secret"),
                Ok(CredentialStatus::EnvironmentSecret) => {
                    fl!("settings-cloud-azure-signed-in-environment")
                }
                Ok(CredentialStatus::Missing) => fl!("settings-cloud-azure-credential-missing"),
                Err(error) => error.to_string(),
            },
        };

        let create =
            ready && configured && !card.refreshing && crate::cloud::unavailable().is_none();

        let account = self
            .cloud_card(crate::cloud::name(CloudProvider::Azure).to_owned())
            .debug_selector(|| "cloud-azure-card".into())
            .child(self.control_note(fl!("settings-cloud-azure-note")))
            .child(div().min_w_0().child(status))
            .children(
                FIELDS
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| *index != VM_SIZE)
                    .map(|(index, (label, _))| {
                        self.cloud_field(
                            i18n::message(label, &[]),
                            card.fields[index].clone().into(),
                        )
                    }),
            )
            .child(self.cloud_field(
                fl!("settings-cloud-azure-client-id"),
                card.client_id.clone().into(),
            ))
            .child(self.cloud_field(
                fl!("settings-cloud-azure-secret"),
                card.secret.clone().into(),
            ))
            .child(
                div().row().justify_end().child(
                    self.cloud_button("cloud-azure-save", fl!("settings-cloud-save"), ready)
                        .when(ready, |button| {
                            button.on_click(cx.listener(|this, _, _, cx| this.save_azure(cx)))
                        }),
                ),
            )
            .when_some(card.message.as_ref(), |account, message| {
                account.child(div().min_w_0().child(message.clone()))
            });

        let vm = self
            .cloud_card(fl!("settings-cloud-azure-new-vm"))
            .debug_selector(|| "cloud-azure-new-vm".into())
            .child(self.render_vm_sizes(card, cx))
            .child(self.azure_install_switch("cloud-azure-install", card.install, ready, cx))
            .children(self.azure_setup_switch("cloud-azure-vm-setup", card.setup, ready, cx))
            .child(
                div().row().justify_end().child(
                    self.cloud_button(
                        "cloud-azure-create",
                        fl!("settings-cloud-azure-create-vm"),
                        create,
                    )
                    .when(create, |button| {
                        button.on_click(cx.listener(|this, _, _, cx| this.create_azure_vm(cx)))
                    }),
                ),
            );

        let sandbox = self
            .cloud_card(fl!("settings-cloud-azure-new-sandbox"))
            .debug_selector(|| "cloud-azure-new-sandbox".into())
            .child(self.render_sandbox_sizes(card, cx))
            .child(self.azure_install_switch(
                "cloud-azure-sandbox-install",
                card.install,
                ready,
                cx,
            ))
            .children(self.azure_setup_switch("cloud-azure-sandbox-setup", card.setup, ready, cx))
            .child(
                div().row().justify_end().child(
                    self.cloud_button(
                        "cloud-azure-create-sandbox",
                        fl!("settings-cloud-azure-create-sandbox"),
                        create,
                    )
                    .when(create, |button| {
                        button.on_click(cx.listener(|this, _, _, cx| this.create_azure_sandbox(cx)))
                    }),
                ),
            );

        let mut devices = self
            .cloud_card(fl!("settings-cloud-azure-devices"))
            .debug_selector(|| "cloud-azure-devices".into());
        let creating = self.azure_job_rows(cx);
        // The last delete's outcome leads, so it reads before an empty list.
        if let Some(outcome) = self.azure_destroy_outcome() {
            devices = devices.child(outcome);
        }
        if card.devices().is_empty() && creating.is_empty() {
            devices = devices.child(self.control_note(fl!("settings-cloud-none-yet")));
        } else if !card.devices().is_empty() {
            devices = devices.child(self.control_note(fl!("settings-cloud-azure-remove-note")));
        }
        devices = devices.children(creating);
        for (index, device) in card.devices().iter().enumerate() {
            let kind = machine_kind(&device.id);
            let group = card.device_group(device).to_owned();
            let detail = match card.device_size(&device.id) {
                Some(size) => fl!(
                    "settings-cloud-azure-device-size",
                    kind = kind,
                    size = size,
                    group = group
                ),
                None => fl!("settings-cloud-azure-device", kind = kind, group = group),
            };
            let actions = self.azure_destroy_actions(
                index,
                device,
                ready && self.azure_removable(device),
                cx,
            );
            devices = devices
                .child(self.cloud_device_row_actions(device, detail, actions))
                .children(self.azure_destroy_detail(device));
        }

        div()
            .flex()
            .flex_col()
            .gap(px(24.))
            .min_w_0()
            .child(account)
            .child(vm)
            .child(sandbox)
            .child(devices)
    }
}

/// What kind of machine a device is, as the messages select on it.
fn machine_kind(id: &str) -> &'static str {
    match Machine::parse(id) {
        Machine::Vm(_) => "vm",
        Machine::Sandbox(_) => "sandbox",
    }
}

#[cfg(test)]
mod tests;
