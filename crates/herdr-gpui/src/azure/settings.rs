//! Validated `[azure]` configuration settings.

use super::{Error, Result};
use crate::{config::AzureConfig, github::Store};
use std::ffi::OsString;

pub(crate) const DEFAULT_RESOURCE_GROUP: &str = "herdr-devices";
pub(crate) const DEFAULT_LOCATION: &str = "centralus";
pub(crate) const DEFAULT_VM_SIZE: &str = "Standard_D2plds_v6";
pub(crate) const DEFAULT_ADMIN_USERNAME: &str = "herdr";
pub(crate) const DEFAULT_SANDBOX_GROUP: &str = "herdr-sandboxes";
pub(crate) const DEFAULT_SANDBOX_IMAGE: &str = "ubuntu";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Settings {
    pub(crate) subscription_id: String,
    pub(crate) resource_group: String,
    pub(crate) location: String,
    pub(crate) vm_size: String,
    pub(crate) admin_username: String,
    pub(crate) sandbox_group: String,
    pub(crate) sandbox_location: String,
    pub(crate) sandbox_image: String,
    pub(crate) sandbox_size: super::sizes::SandboxSize,
    pub(crate) tenant_id: Option<String>,
    pub(crate) client_id: Option<String>,
    pub(crate) use_azure_cli: bool,
    pub(crate) store: Store,
    pub(crate) auto_stop: Option<std::time::Duration>,
}

/// Whether resolving may ask the Azure CLI for its signed-in account. `az`
/// takes most of a second, so checking a config file never runs it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Cli {
    Ask,
    Skip,
}

/// Stands in for the subscription `az` would report, so a file check still
/// validates the other keys.
const UNASKED_SUBSCRIPTION: &str = "00000000-0000-0000-0000-000000000000";

impl Settings {
    pub(crate) fn resolve(
        config: &AzureConfig,
        var: impl Fn(&str) -> Option<OsString>,
    ) -> Result<Option<Self>> {
        Self::resolve_with(config, var, Cli::Ask)
    }

    /// Validate `config` as `resolve` would, without running `az`.
    pub(crate) fn check(
        config: &AzureConfig,
        var: impl Fn(&str) -> Option<OsString>,
    ) -> Result<()> {
        Self::resolve_with(config, var, Cli::Skip).map(drop)
    }

    fn resolve_with(
        config: &AzureConfig,
        var: impl Fn(&str) -> Option<OsString>,
        cli: Cli,
    ) -> Result<Option<Self>> {
        let env_string = |name| -> Result<Option<String>> {
            match var(name) {
                Some(val) => Ok(Some(val.into_string().map_err(|_| {
                    Error::Encoding(Box::leak(name.to_owned().into_boxed_str()))
                })?)),
                None => Ok(None),
            }
        };

        let subscription =
            env_string("HERDR_AZURE_SUBSCRIPTION_ID")?.or_else(|| config.subscription_id.clone());

        let tenant = env_string("HERDR_AZURE_TENANT_ID")?.or_else(|| config.tenant_id.clone());
        let client_id = env_string("HERDR_AZURE_CLIENT_ID")?.or_else(|| config.client_id.clone());

        let use_cli_env = var("HERDR_AZURE_USE_CLI").is_some();
        let explicit_cli = config.use_azure_cli.unwrap_or(false) || use_cli_env;

        let has_subscription = subscription
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty());
        let has_client_id = client_id.as_deref().is_some_and(|c| !c.trim().is_empty());

        if !has_subscription && !has_client_id && !explicit_cli {
            return Ok(None);
        }

        let use_cli = config.use_azure_cli.unwrap_or_else(|| {
            !has_client_id && (cli == Cli::Skip || super::cli::is_az_available())
        });

        // Resolve subscription: if missing and CLI is available, try auto-resolving
        let subscription_id = match subscription {
            Some(s) if !s.trim().is_empty() => s.trim().to_owned(),
            _ if use_cli && cli == Cli::Skip => UNASKED_SUBSCRIPTION.to_owned(),
            _ if use_cli => match super::cli::default_subscription() {
                Ok(s) => s,
                Err(_) => return Ok(None),
            },
            _ => return Ok(None),
        };

        if !is_guid(&subscription_id) {
            return Err(Error::Field("azure.subscription_id"));
        }

        let tenant_id = match tenant {
            Some(t) if !t.trim().is_empty() => {
                let t = t.trim().to_owned();
                if !is_guid(&t) {
                    return Err(Error::Field("azure.tenant_id"));
                }
                Some(t)
            }
            _ if use_cli && cli == Cli::Ask => super::cli::default_tenant().ok(),
            _ => None,
        };

        let client_id = match client_id {
            Some(c) if !c.trim().is_empty() => {
                let c = c.trim().to_owned();
                if !is_guid(&c) {
                    return Err(Error::Field("azure.client_id"));
                }
                Some(c)
            }
            _ => None,
        };

        let resource_group = env_string("HERDR_AZURE_RESOURCE_GROUP")?
            .or_else(|| config.resource_group.clone())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_RESOURCE_GROUP.to_owned());

        let location = env_string("HERDR_AZURE_LOCATION")?
            .or_else(|| config.location.clone())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_LOCATION.to_owned());

        let vm_size = env_string("HERDR_AZURE_VM_SIZE")?
            .or_else(|| config.vm_size.clone())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_VM_SIZE.to_owned());

        let admin_username = env_string("HERDR_AZURE_ADMIN_USERNAME")?
            .or_else(|| config.admin_username.clone())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_ADMIN_USERNAME.to_owned());

        // These become URL path segments and a host name, so they are checked
        // here rather than escaped later.
        let sandbox_group = env_string("HERDR_AZURE_SANDBOX_GROUP")?
            .or_else(|| config.sandbox_group.clone())
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_owned())
            .unwrap_or_else(|| DEFAULT_SANDBOX_GROUP.to_owned());
        if !is_segment(&sandbox_group) {
            return Err(Error::Field("azure.sandbox_group"));
        }

        let sandbox_location = env_string("HERDR_AZURE_SANDBOX_LOCATION")?
            .or_else(|| config.sandbox_location.clone())
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_ascii_lowercase())
            // Sandboxes run in most VM regions, so they follow the VMs unless
            // set apart.
            .unwrap_or_else(|| location.to_ascii_lowercase().replace(' ', ""));
        if sandbox_location.is_empty()
            || !sandbox_location
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        {
            return Err(Error::Field("azure.sandbox_location"));
        }

        let sandbox_image = env_string("HERDR_AZURE_SANDBOX_IMAGE")?
            .or_else(|| config.sandbox_image.clone())
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_owned())
            .unwrap_or_else(|| DEFAULT_SANDBOX_IMAGE.to_owned());
        if !is_segment(&sandbox_image) {
            return Err(Error::Field("azure.sandbox_image"));
        }

        let sandbox_size = match env_string("HERDR_AZURE_SANDBOX_CPU")?
            .or_else(|| config.sandbox_cpu.clone())
            .filter(|s| !s.trim().is_empty())
        {
            Some(cores) => {
                super::sizes::SandboxSize::parse(&cores).ok_or(Error::Field("azure.sandbox_cpu"))?
            }
            None => super::sizes::SandboxSize::standard(super::sizes::DEFAULT_SANDBOX_MILLICORES),
        };

        let auto_stop_raw = env_string("HERDR_AZURE_AUTO_STOP")?
            .or_else(|| config.auto_stop.clone())
            .unwrap_or_else(|| "30m".to_owned());
        let auto_stop = parse_auto_stop(&auto_stop_raw)?;

        Ok(Some(Self {
            subscription_id,
            resource_group,
            location,
            vm_size,
            admin_username,
            sandbox_group,
            sandbox_location,
            sandbox_image,
            sandbox_size,
            tenant_id,
            client_id,
            use_azure_cli: use_cli,
            store: Store::for_policy(config.allow_plaintext_credentials),
            auto_stop,
        }))
    }

    /// The `account` a VM saved now records: its subscription and resource
    /// group, so changing the group in Settings cannot redirect the device.
    pub(crate) fn vm_account(&self) -> String {
        format!("{}/{}", self.subscription_id, self.resource_group)
    }

    /// The `account` a sandbox saved now records: its resource group, and the
    /// sandbox group and region whose data plane reaches it.
    pub(crate) fn sandbox_account(&self) -> String {
        format!(
            "{}/{}/{}/{}",
            self.subscription_id, self.resource_group, self.sandbox_group, self.sandbox_location
        )
    }

    /// Whether a device saved under `account` is in this subscription.
    pub(crate) fn owns(&self, account: &str) -> bool {
        account.split('/').next() == Some(self.subscription_id.as_str())
    }

    /// These settings pointed at where a device saved under `account` lives.
    /// A device saved before devices recorded their group has only the
    /// subscription, and follows the configured groups as it always did.
    pub(crate) fn for_device(&self, account: &str) -> Result<Self> {
        if !self.owns(account) {
            return Err(Error::Account);
        }
        let place: Vec<&str> = account.split('/').skip(1).collect();
        if place.iter().any(|part| part.is_empty()) {
            return Err(Error::Field("saved Azure device account"));
        }
        let mut scoped = self.clone();
        match place[..] {
            [] => {}
            [resource_group] => scoped.resource_group = resource_group.to_owned(),
            [resource_group, sandbox_group, sandbox_location] => {
                if !is_segment(sandbox_group) || !is_segment(sandbox_location) {
                    return Err(Error::Field("saved Azure device account"));
                }
                scoped.resource_group = resource_group.to_owned();
                scoped.sandbox_group = sandbox_group.to_owned();
                scoped.sandbox_location = sandbox_location.to_owned();
            }
            _ => return Err(Error::Field("saved Azure device account")),
        }
        Ok(scoped)
    }

    /// The resource group a device saved under `account` is in, for display.
    pub(crate) fn group_of<'a>(&'a self, account: &'a str) -> &'a str {
        account
            .split('/')
            .nth(1)
            .filter(|group| !group.is_empty())
            .unwrap_or(&self.resource_group)
    }
}

pub(crate) fn parse_auto_stop(text: &str) -> Result<Option<std::time::Duration>> {
    let text = text.trim();
    if text.is_empty() || text.eq_ignore_ascii_case("never") {
        return Ok(None);
    }
    if let Some(min) = text.strip_suffix('m').or_else(|| text.strip_suffix('M')) {
        let mins: u64 = min
            .trim()
            .parse()
            .map_err(|_| Error::Field("azure.auto_stop"))?;
        if mins == 0 {
            return Err(Error::Field("azure.auto_stop"));
        }
        return Ok(Some(std::time::Duration::from_secs(mins * 60)));
    }
    if let Some(hrs) = text.strip_suffix('h').or_else(|| text.strip_suffix('H')) {
        let hours: u64 = hrs
            .trim()
            .parse()
            .map_err(|_| Error::Field("azure.auto_stop"))?;
        if hours == 0 {
            return Err(Error::Field("azure.auto_stop"));
        }
        return Ok(Some(std::time::Duration::from_secs(hours * 3600)));
    }
    if let Some(secs) = text.strip_suffix('s').or_else(|| text.strip_suffix('S')) {
        let seconds: u64 = secs
            .trim()
            .parse()
            .map_err(|_| Error::Field("azure.auto_stop"))?;
        if seconds == 0 {
            return Err(Error::Field("azure.auto_stop"));
        }
        return Ok(Some(std::time::Duration::from_secs(seconds)));
    }
    let seconds: u64 = text.parse().map_err(|_| Error::Field("azure.auto_stop"))?;
    if seconds == 0 {
        return Err(Error::Field("azure.auto_stop"));
    }
    Ok(Some(std::time::Duration::from_secs(seconds)))
}

/// A name Azure accepts as one resource path segment.
pub(crate) fn is_segment(value: &str) -> bool {
    (1..=63).contains(&value.len())
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && value != "."
        && value != ".."
}

fn is_guid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok()
}
