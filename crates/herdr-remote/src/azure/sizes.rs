//! What new Azure machines can be, and what each costs. Settings offers a
//! short list of VM sizes, with any other size typed in, and the sandbox
//! sizes the service accepts. Specs come from the region's own size list and
//! prices from Azure's public Retail Prices API, both read on a worker; a
//! price that cannot be read is left out rather than guessed.
//!
//! Prices are Linux list prices in US dollars, before discounts or credits.
//! A new VM runs as Spot, with its OS disk on the host's local storage, so it
//! is billed only while it exists, at the Spot price, with no disk to pay
//! for; its pay-as-you-go price is shown beside it. A sandbox is billed for
//! CPU and memory while it runs.

use super::{
    Result, Settings,
    http::{self, Client},
};
use crate::cloud::SavedDevice;
use serde::Deserialize;
use std::collections::HashMap;

/// VM sizes Settings lists, small to large: Arm v6 sizes with local NVMe,
/// which the OS disk lives on (see [`ephemeral_placement`]). Arm undercuts
/// x64 at every size, and v6 sizes run their full CPU rather than bursting.
/// Sizes with under 4 GiB cannot hold an agent and Herdr together.
pub const VM_CHOICES: &[&str] = &[
    "Standard_D2plds_v6",
    "Standard_D2pds_v6",
    "Standard_D4plds_v6",
    "Standard_D4pds_v6",
    "Standard_D8pds_v6",
];

/// Sandbox CPU in millicores. Memory is the most the service allows for it,
/// 2 GiB per core, and the service gives 20 GiB of disk per core.
pub const SANDBOX_MILLICORES: &[u32] = &[500, 1000, 2000, 4000, 8000];
pub const DEFAULT_SANDBOX_MILLICORES: u32 = 1000;
const HOURS_PER_MONTH: f64 = 730.0;
const COMPUTE_API_VERSION: &str = "2024-07-01";
const PRICES: &str = "https://prices.azure.com/api/retail/prices";

/// A sandbox size as the service takes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SandboxSize {
    pub millicores: u32,
    pub memory_mib: u32,
}

impl SandboxSize {
    /// `millicores` with the most memory the service allows for them.
    pub fn standard(millicores: u32) -> Self {
        Self {
            millicores,
            memory_mib: millicores * 2048 / 1000,
        }
    }

    /// The size a sandbox reports, as `1000m` and `2048Mi`.
    pub fn reported(cpu: &str, memory: &str) -> Option<Self> {
        let millicores = cpu.strip_suffix('m')?.parse().ok()?;
        let memory_mib = match memory.strip_suffix("Mi") {
            Some(mib) => mib.parse().ok()?,
            None => memory
                .strip_suffix("Gi")?
                .parse::<u32>()
                .ok()?
                .checked_mul(1024)?,
        };
        Some(Self {
            millicores,
            memory_mib,
        })
    }

    pub fn disk_gib(self) -> u32 {
        self.millicores * 20 / 1000
    }

    pub fn cpu(self) -> String {
        format!("{}m", self.millicores)
    }

    pub fn memory(self) -> String {
        format!("{}Mi", self.memory_mib)
    }

    /// How many cores, as Settings and the config write them: `0.5`, `2`.
    pub fn cores(self) -> String {
        if self.millicores.is_multiple_of(1000) {
            (self.millicores / 1000).to_string()
        } else {
            format!("{}", f64::from(self.millicores) / 1000.0)
        }
    }

    /// The size for `text` cores, if it is one the service is offered at.
    pub fn parse(text: &str) -> Option<Self> {
        let cores: f64 = text.trim().parse().ok()?;
        let millicores = (cores * 1000.0).round();
        SANDBOX_MILLICORES
            .iter()
            .find(|&&choice| f64::from(choice) == millicores)
            .map(|&millicores| Self::standard(millicores))
    }
}

/// The feature letters after a size's vCPU count, as `pls` in
/// `Standard_D2pls_v5`, and its version, as 5.
fn features(size: &str) -> (&str, u32) {
    let name = size.strip_prefix("Standard_").unwrap_or(size);
    let mut parts = name.split('_');
    let family = parts.next().unwrap_or(name);
    let version = parts
        .find_map(|part| part.strip_prefix('v')?.parse().ok())
        .unwrap_or(1);
    let letters = match family.find(|c: char| c.is_ascii_digit()) {
        Some(digits) => {
            family[digits..].trim_start_matches(|c: char| c.is_ascii_digit() || c == '-')
        }
        None => "",
    };
    (letters, version)
}

/// Whether a VM size runs on Arm: Azure marks Arm sizes with a lowercase `p`
/// among the feature letters after the vCPU count, as in `B2pts_v2`.
pub fn is_arm(size: &str) -> bool {
    features(size).0.contains('p')
}

/// Where a size keeps an ephemeral OS disk, as the template's `placement`:
/// the local NVMe of a v6 or later size with a `d`, the temp disk of an
/// older one. `None` for a size with no local storage, which cannot host
/// one. Azure checks the disk fits when the VM is created.
pub fn ephemeral_placement(size: &str) -> Option<&'static str> {
    let (letters, version) = features(size);
    if !letters.contains('d') {
        return None;
    }
    Some(if version >= 6 {
        "NvmeDisk"
    } else {
        "ResourceDisk"
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
pub struct VmSpec {
    #[serde(rename = "numberOfCores")]
    pub cores: u32,
    #[serde(rename = "memoryInMB")]
    pub memory_mb: u64,
}

/// An hourly list price in US dollars.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Price(pub f64);

impl Price {
    /// What the price comes to over a month of 730 running hours.
    pub fn monthly(self) -> f64 {
        self.0 * HOURS_PER_MONTH
    }
}

/// Container Apps rates that sandboxes are billed at while they run.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SandboxRates {
    pub vcpu_second: f64,
    pub gib_second: f64,
}

impl SandboxRates {
    pub fn hourly(self, size: SandboxSize) -> Price {
        let cores = f64::from(size.millicores) / 1000.0;
        let gib = f64::from(size.memory_mib) / 1024.0;
        Price((cores * self.vcpu_second + gib * self.gib_second) * 3600.0)
    }
}

/// Specs and prices for the configured regions.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Catalog {
    pub location: String,
    pub sandbox_location: String,
    /// Every size the VM region offers.
    pub vms: HashMap<String, VmSpec>,
    /// Pay-as-you-go prices, for comparison.
    pub vm_prices: HashMap<String, Price>,
    /// What a new VM is billed at. Spot prices move with spare capacity, so
    /// these are as of when the catalog was read.
    pub spot_prices: HashMap<String, Price>,
    pub sandbox_rates: Option<SandboxRates>,
}

/// Read what the regions offer and cost. Blocks on the network: call it on a
/// worker. Prices are best effort; specs need the account.
pub fn catalog(settings: &Settings) -> Result<Catalog> {
    let client = Client::new(settings)?;
    let vms = region_sizes(&client, settings)?;
    let mut sizes: Vec<&str> = VM_CHOICES.to_vec();
    if !sizes.contains(&settings.vm_size.as_str()) && vms.contains_key(&settings.vm_size) {
        sizes.push(&settings.vm_size);
    }
    let (vm_prices, spot_prices) = vm_prices(&settings.location, &sizes).unwrap_or_else(|error| {
        tracing::warn!(category = "azure_prices", %error, "Could not read Azure VM prices");
        (HashMap::new(), HashMap::new())
    });
    let sandbox_rates = sandbox_rates(&settings.sandbox_location).unwrap_or_else(|error| {
        tracing::warn!(category = "azure_prices", %error, "Could not read Azure sandbox prices");
        None
    });
    Ok(Catalog {
        location: settings.location.clone(),
        sandbox_location: settings.sandbox_location.clone(),
        vms,
        vm_prices,
        spot_prices,
        sandbox_rates,
    })
}

#[derive(Deserialize)]
struct SizeList {
    #[serde(default)]
    value: Vec<NamedSize>,
}

#[derive(Deserialize)]
struct NamedSize {
    name: String,
    #[serde(flatten)]
    spec: VmSpec,
}

fn region_sizes(client: &Client, settings: &Settings) -> Result<HashMap<String, VmSpec>> {
    let path = format!(
        "/subscriptions/{}/providers/Microsoft.Compute/locations/{}/vmSizes?api-version={COMPUTE_API_VERSION}",
        settings.subscription_id, settings.location
    );
    let list: SizeList = client.get(&path)?;
    Ok(list
        .value
        .into_iter()
        .map(|size| (size.name, size.spec))
        .collect())
}

#[derive(Deserialize)]
struct PriceList {
    #[serde(rename = "Items", default)]
    items: Vec<PriceItem>,
}

#[derive(Clone, Deserialize)]
pub struct PriceItem {
    #[serde(rename = "armSkuName", default)]
    pub sku: String,
    #[serde(rename = "skuName", default)]
    pub sku_name: String,
    #[serde(rename = "productName", default)]
    pub product: String,
    #[serde(rename = "meterName", default)]
    pub meter: String,
    #[serde(rename = "retailPrice", default)]
    pub price: f64,
    #[serde(rename = "currencyCode", default)]
    pub currency: String,
}

/// Names that can sit inside a quoted OData string as they are.
fn plain(text: &str) -> bool {
    !text.is_empty()
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

/// Each size's pay-as-you-go and Spot prices.
type Prices = (HashMap<String, Price>, HashMap<String, Price>);

fn vm_prices(location: &str, sizes: &[&str]) -> Result<Prices> {
    let sizes: Vec<&str> = sizes.iter().copied().filter(|size| plain(size)).collect();
    if sizes.is_empty() || !plain(location) {
        return Ok((HashMap::new(), HashMap::new()));
    }
    let skus = sizes
        .iter()
        .map(|size| format!("armSkuName eq '{size}'"))
        .collect::<Vec<_>>()
        .join(" or ");
    let filter = format!(
        "serviceName eq 'Virtual Machines' and priceType eq 'Consumption' and armRegionName eq '{location}' and ({skus})"
    );
    let list: PriceList = http::get_public(PRICES, &[("$filter", &filter)])?;
    Ok((linux_prices(list.items.clone()), spot_prices(list.items)))
}

/// The Linux pay-as-you-go price of each size: not Windows, Spot, Low
/// Priority, or Cloud Services.
pub fn linux_prices(items: Vec<PriceItem>) -> HashMap<String, Price> {
    items
        .into_iter()
        .filter(|item| {
            item.currency == "USD"
                && item.product.starts_with("Virtual Machines")
                && !item.product.ends_with("Windows")
                && !item.sku_name.contains("Spot")
                && !item.sku_name.contains("Low Priority")
        })
        .map(|item| (item.sku, Price(item.price)))
        .collect()
}

/// The Linux Spot price of each size.
pub fn spot_prices(items: Vec<PriceItem>) -> HashMap<String, Price> {
    items
        .into_iter()
        .filter(|item| {
            item.currency == "USD"
                && item.product.starts_with("Virtual Machines")
                && !item.product.ends_with("Windows")
                && item.sku_name.ends_with(" Spot")
        })
        .map(|item| (item.sku, Price(item.price)))
        .collect()
}

fn sandbox_rates(location: &str) -> Result<Option<SandboxRates>> {
    if !plain(location) {
        return Ok(None);
    }
    let filter = format!(
        "serviceName eq 'Azure Container Apps' and priceType eq 'Consumption' and armRegionName eq '{location}' and (meterName eq 'Standard vCPU Active Usage' or meterName eq 'Standard Memory Active Usage')"
    );
    let list: PriceList = http::get_public(PRICES, &[("$filter", &filter)])?;
    Ok(rates(&list.items))
}

pub fn rates(items: &[PriceItem]) -> Option<SandboxRates> {
    let rate = |meter: &str| {
        items
            .iter()
            .find(|item| item.meter == meter && item.currency == "USD")
            .map(|item| item.price)
    };
    Some(SandboxRates {
        vcpu_second: rate("Standard vCPU Active Usage")?,
        gib_second: rate("Standard Memory Active Usage")?,
    })
}

/// A saved machine's size, which Settings writes out in the window's
/// language.
#[derive(Clone, Debug, PartialEq)]
pub enum DeviceSize {
    /// A VM: Azure's size name, its specs when the region lists them, and
    /// whether it runs as Spot.
    Vm {
        size: String,
        spec: Option<VmSpec>,
        spot: bool,
    },
    Sandbox(SandboxSize),
}

/// Each saved device's size, as Settings lists it, by device ID. Best
/// effort: a device whose machine is gone or unreadable is left out.
pub fn device_sizes(
    settings: &Settings,
    devices: &[SavedDevice],
    catalog: &Catalog,
) -> HashMap<String, DeviceSize> {
    use super::machine::Machine;
    let mut sizes = HashMap::new();
    let vms = devices
        .iter()
        .any(|device| matches!(Machine::parse(&device.id), Machine::Vm(_)));
    if vms && let Ok(found) = Client::new(settings).and_then(|client| super::api::vm_sizes(&client))
    {
        for device in devices {
            if let Machine::Vm(name) = Machine::parse(&device.id)
                && let Some(vm) = found.get(&name.to_ascii_lowercase())
            {
                sizes.insert(
                    device.id.clone(),
                    DeviceSize::Vm {
                        spec: catalog.vms.get(&vm.size).copied(),
                        size: vm.size.clone(),
                        spot: vm.spot,
                    },
                );
            }
        }
    }
    let sandboxes = devices
        .iter()
        .any(|device| matches!(Machine::parse(&device.id), Machine::Sandbox(_)));
    if sandboxes && let Ok(found) = super::sandbox::sizes(settings) {
        for device in devices {
            if let Machine::Sandbox(id) = Machine::parse(&device.id)
                && let Some(size) = found.get(id)
            {
                sizes.insert(device.id.clone(), DeviceSize::Sandbox(*size));
            }
        }
    }
    sizes
}

#[cfg(test)]
mod tests;
