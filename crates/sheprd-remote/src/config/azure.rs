//! The `[azure]` table: an Azure account whose VMs become devices. A build
//! without the `azure` feature still parses the table, so one config file serves
//! every build, but never reads it. The client secret is kept in the credential store.

#[cfg(feature = "azure")]
use crate::Result;
use serde::Deserialize;
#[cfg(feature = "azure")]
use std::{env, ffi::OsString};

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AzureConfig {
    /// Azure subscription ID (UUID).
    pub subscription_id: Option<String>,
    /// Resource group to deploy VMs into (default "herdr-devices").
    pub resource_group: Option<String>,
    /// Azure region / location (default "centralus").
    pub location: Option<String>,
    /// VM SKU / size (default "Standard_D2plds_v6"). A size with local storage,
    /// a `d` in its name, keeps the OS disk there; another gets a managed disk.
    pub vm_size: Option<String>,
    /// Admin username on the VM (default "herdr").
    pub admin_username: Option<String>,
    /// Container Apps sandbox group that holds sandboxes (default "herdr-sandboxes").
    pub sandbox_group: Option<String>,
    /// Region of the sandbox group and its data plane (default: `location`).
    pub sandbox_location: Option<String>,
    /// Public disk image new sandboxes boot from (default "ubuntu").
    pub sandbox_image: Option<String>,
    /// vCPUs of new sandboxes: 0.5, 1, 2, 4, or 8 (default 1). Each gets
    /// 2 GiB of memory and 20 GiB of disk per vCPU.
    pub sandbox_cpu: Option<String>,
    /// Azure tenant ID (UUID).
    pub tenant_id: Option<String>,
    /// Azure Service Principal client / application ID (UUID).
    pub client_id: Option<String>,
    /// Explicitly use the local Azure CLI (`az`) for authentication.
    pub use_azure_cli: Option<bool>,
    /// `never` or how long a VM may go without a connection (e.g. `15m`, `30m`,
    /// `1h`, `2h`, or seconds like `1800`) before it stops billing: a Spot VM
    /// with an ephemeral disk is deleted, an older VM deallocated. Default `30m`.
    pub auto_stop: Option<String>,
    pub allow_plaintext_credentials: bool,
}

#[cfg(feature = "azure")]
impl AzureConfig {
    /// The validated account settings, or `None` when Azure is not set up.
    pub fn settings(&self) -> Result<Option<crate::azure::Settings>> {
        self.settings_with(|name| env::var_os(name))
    }

    pub fn settings_with(
        &self,
        var: impl Fn(&str) -> Option<OsString>,
    ) -> Result<Option<crate::azure::Settings>> {
        Ok(crate::azure::Settings::resolve(self, var)?)
    }

    /// Validate the table without asking `az` for the signed-in account.
    pub fn check(&self) -> Result<()> {
        Ok(crate::azure::Settings::check(self, |_| None)?)
    }

    /// Whether an Azure account is configured (via config or environment).
    pub fn configured(&self, var: impl Fn(&str) -> Option<OsString>) -> bool {
        self.subscription_id.is_some()
            || var("HERDR_AZURE_SUBSCRIPTION_ID").is_some()
            || self.client_id.is_some()
            || var("HERDR_AZURE_CLIENT_ID").is_some()
            || self.use_azure_cli == Some(true)
    }
}

/// The `[azure]` keys the Settings window edits. An empty value removes its key.
#[cfg(feature = "azure")]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AzureFields {
    pub subscription_id: String,
    pub resource_group: String,
    pub location: String,
    pub vm_size: String,
    pub auto_stop: String,
    pub tenant_id: String,
    pub client_id: String,
    pub sandbox_group: String,
    pub sandbox_location: String,
    pub sandbox_image: String,
    pub sandbox_cpu: String,
}

#[cfg(feature = "azure")]
impl AzureFields {
    pub fn from_config(config: &AzureConfig) -> Self {
        let text = |value: &Option<String>| value.clone().unwrap_or_default();
        Self {
            subscription_id: text(&config.subscription_id),
            resource_group: text(&config.resource_group),
            location: text(&config.location),
            vm_size: text(&config.vm_size),
            auto_stop: config.auto_stop.clone().unwrap_or_else(|| "30m".into()),
            tenant_id: text(&config.tenant_id),
            client_id: text(&config.client_id),
            sandbox_group: text(&config.sandbox_group),
            sandbox_location: text(&config.sandbox_location),
            sandbox_image: text(&config.sandbox_image),
            sandbox_cpu: text(&config.sandbox_cpu),
        }
    }

    pub fn entries(&self) -> [(&'static str, &str); 11] {
        [
            ("subscription_id", &self.subscription_id),
            ("resource_group", &self.resource_group),
            ("location", &self.location),
            ("vm_size", &self.vm_size),
            ("auto_stop", &self.auto_stop),
            ("tenant_id", &self.tenant_id),
            ("client_id", &self.client_id),
            ("sandbox_group", &self.sandbox_group),
            ("sandbox_location", &self.sandbox_location),
            ("sandbox_image", &self.sandbox_image),
            ("sandbox_cpu", &self.sandbox_cpu),
        ]
    }

    pub fn validate(&self, existing: &AzureConfig) -> Result<()> {
        let value = |text: &str| {
            let text = text.trim();
            (!text.is_empty()).then(|| text.to_owned())
        };
        let config = AzureConfig {
            subscription_id: value(&self.subscription_id),
            resource_group: value(&self.resource_group),
            location: value(&self.location),
            vm_size: value(&self.vm_size),
            auto_stop: value(&self.auto_stop),
            tenant_id: value(&self.tenant_id),
            client_id: value(&self.client_id),
            sandbox_group: value(&self.sandbox_group),
            sandbox_location: value(&self.sandbox_location),
            sandbox_image: value(&self.sandbox_image),
            sandbox_cpu: value(&self.sandbox_cpu),
            ..existing.clone()
        };
        config.check()
    }
}
