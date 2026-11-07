//! Azure Resource Manager REST API calls for Virtual Machines and resources.

use super::{Error, Result, Settings, http::Client};
use serde::Deserialize;
use std::time::{Duration, Instant};

const API_VERSION_RESOURCES: &str = "2021-04-01";
/// 2024-03-01 or later, for an ephemeral OS disk on local NVMe.
const API_VERSION_COMPUTE: &str = "2024-07-01";
// Managed disks have their own version set, separate from virtual machines.
const API_VERSION_DISKS: &str = "2024-03-02";
const API_VERSION_NETWORK: &str = "2023-09-01";
const POLL_INTERVAL: Duration = Duration::from_secs(3);
const TIMEOUT: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerState {
    Running,
    Stopped,
    Deallocated,
    Starting,
    Unknown,
}

#[derive(Clone, Debug, Deserialize)]
struct VmSummary {
    name: String,
    #[serde(default)]
    properties: Option<VmProperties>,
}

#[derive(Clone, Debug, Deserialize)]
struct VmProperties {
    #[serde(rename = "hardwareProfile", default)]
    hardware: Option<HardwareProfile>,
    #[serde(default)]
    priority: Option<String>,
}

/// A VM's size and whether it runs as Spot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VmModel {
    pub size: String,
    pub spot: bool,
}

#[derive(Clone, Debug, Deserialize)]
struct HardwareProfile {
    #[serde(rename = "vmSize", default)]
    size: Option<String>,
}

#[derive(Deserialize)]
struct VmList {
    #[serde(default)]
    value: Vec<VmSummary>,
}

#[derive(Deserialize)]
struct InstanceView {
    #[serde(default)]
    statuses: Vec<StatusItem>,
}

#[derive(Deserialize)]
struct StatusItem {
    #[serde(default)]
    code: String,
}

#[derive(Deserialize)]
struct PublicIpResponse {
    properties: Option<PublicIpProperties>,
}

#[derive(Deserialize)]
struct PublicIpProperties {
    #[serde(rename = "ipAddress")]
    ip_address: Option<String>,
}

/// Create the resource group in `location` if it is missing. An existing
/// group is left where it is: Azure refuses to move one, and its resources
/// may live in any region.
pub fn ensure_resource_group(client: &Client, rg: &str, location: &str) -> Result<()> {
    let path = format!(
        "/subscriptions/{}/resourcegroups/{}?api-version={}",
        client.settings.subscription_id, rg, API_VERSION_RESOURCES
    );
    match client.get::<serde_json::Value>(&path) {
        Ok(_) => return Ok(()),
        Err(Error::Status(404, _)) => {}
        Err(error) => return Err(error),
    }
    let body = serde_json::json!({
        "location": location
    });
    let _: serde_json::Value = client.put(&path, &body)?;
    Ok(())
}

/// How a VM is billed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Priority {
    /// Spare capacity at a fraction of the price, which Azure may take back.
    Spot,
    /// Pay-as-you-go.
    Regular,
}

/// Deploy VM `vm_name` as Spot, or at the regular price when Azure has no
/// Spot capacity to give: some subscriptions, Visual Studio ones among them,
/// are never offered Spot. Returns the priority it got.
pub fn deploy_vm(client: &Client, vm_name: &str, ssh_public_key: &str) -> Result<Priority> {
    ensure_resource_group(
        client,
        &client.settings.resource_group,
        &client.settings.location,
    )?;
    match deploy(client, vm_name, ssh_public_key, Priority::Spot) {
        Err(Error::SkuNotAvailable(reason)) => {
            tracing::info!(
                category = "azure_vm",
                vm_name,
                %reason,
                "No Spot capacity for this VM; creating it at the regular price"
            );
            deploy(client, vm_name, ssh_public_key, Priority::Regular)?;
            Ok(Priority::Regular)
        }
        deployed => deployed.map(|()| Priority::Spot),
    }
}

fn deploy(client: &Client, vm_name: &str, ssh_public_key: &str, priority: Priority) -> Result<()> {
    let template = vm_template(client.settings, vm_name, ssh_public_key, priority);

    let deployment_name = format!("herdr-deploy-{}", vm_name);
    let path = format!(
        "/subscriptions/{}/resourcegroups/{}/providers/Microsoft.Resources/deployments/{}?api-version={}",
        client.settings.subscription_id,
        client.settings.resource_group,
        deployment_name,
        API_VERSION_RESOURCES
    );

    let body = serde_json::json!({
        "properties": {
            "mode": "Incremental",
            "template": template
        }
    });

    let _: serde_json::Value = client.put(&path, &body)?;
    Ok(())
}

/// The deployment template of VM `vm_name`. A device is disposable: its
/// work lives in Git, not on its disk. On a size with local storage its OS
/// disk lives there, which is faster than a managed disk and not billed
/// apart from the VM; such a VM cannot be stopped and kept, only deleted,
/// which is what auto-stop then does. A size without local storage gets a
/// Standard SSD managed disk instead, deallocated when idle. Its NIC, public
/// IP, and disk go with it, from here or the portal.
pub fn vm_template(
    settings: &Settings,
    vm_name: &str,
    ssh_public_key: &str,
    priority: Priority,
) -> serde_json::Value {
    let placement = super::sizes::ephemeral_placement(&settings.vm_size);
    let os_disk = match placement {
        Some(placement) => serde_json::json!({
            "name": format!("{vm_name}-osdisk"),
            "caching": "ReadOnly",
            "createOption": "FromImage",
            "deleteOption": "Delete",
            "diffDiskSettings": { "option": "Local", "placement": placement }
        }),
        // Standard SSD: the disk bills even while the VM is deallocated,
        // and Premium costs about twice as much for a disk a device rarely
        // taxes.
        None => serde_json::json!({
            "name": format!("{vm_name}-osdisk"),
            "caching": "ReadWrite",
            "createOption": "FromImage",
            "deleteOption": "Delete",
            "managedDisk": { "storageAccountType": "StandardSSD_LRS" }
        }),
    };
    let mut vm = serde_json::json!({
        "hardwareProfile": { "vmSize": settings.vm_size },
        "osProfile": {
            "computerName": vm_name,
            "adminUsername": settings.admin_username,
            "linuxConfiguration": {
                "disablePasswordAuthentication": true,
                "ssh": {
                    "publicKeys": [
                        {
                            "path": format!("/home/{}/.ssh/authorized_keys", settings.admin_username),
                            "keyData": ssh_public_key
                        }
                    ]
                }
            }
        },
        "storageProfile": {
            "imageReference": image(&settings.vm_size),
            "osDisk": os_disk
        },
        "networkProfile": {
            "networkInterfaces": [
                {
                    "id": format!("[resourceId('Microsoft.Network/networkInterfaces', '{vm_name}-nic')]"),
                    "properties": { "deleteOption": "Delete" }
                }
            ]
        }
    });
    if priority == Priority::Spot {
        // An ephemeral OS disk cannot be deallocated, so such a VM is deleted
        // when evicted; -1 pays up to the pay-as-you-go price rather than
        // being evicted over price.
        vm["priority"] = "Spot".into();
        vm["evictionPolicy"] = if placement.is_some() {
            "Delete"
        } else {
            "Deallocate"
        }
        .into();
        vm["billingProfile"] = serde_json::json!({ "maxPrice": -1 });
    }
    network_template(settings, vm_name, vm)
}

/// Ubuntu 22.04 for the size's architecture.
fn image(vm_size: &str) -> serde_json::Value {
    let sku = if super::sizes::is_arm(vm_size) {
        "22_04-lts-arm64"
    } else {
        "22_04-lts-gen2"
    };
    serde_json::json!({
        "publisher": "Canonical",
        "offer": "0001-com-ubuntu-server-jammy",
        "sku": sku,
        "version": "latest"
    })
}

/// The template around a VM's `properties`: its NSG, VNet, IP, and NIC.
fn network_template(
    settings: &Settings,
    vm_name: &str,
    vm: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "$schema": "https://schema.management.azure.com/schemas/2019-04-01/deploymentTemplate.json#",
        "contentVersion": "1.0.0.0",
        "resources": [
            {
                "type": "Microsoft.Network/networkSecurityGroups",
                "apiVersion": API_VERSION_NETWORK,
                "name": format!("{}-nsg", vm_name),
                "location": settings.location,
                "properties": {
                    "securityRules": [
                        {
                            "name": "SSH",
                            "properties": {
                                "protocol": "Tcp",
                                "sourcePortRange": "*",
                                "destinationPortRange": "22",
                                "sourceAddressPrefix": "*",
                                "destinationAddressPrefix": "*",
                                "access": "Allow",
                                "priority": 1000,
                                "direction": "Inbound"
                            }
                        }
                    ]
                }
            },
            {
                "type": "Microsoft.Network/virtualNetworks",
                "apiVersion": API_VERSION_NETWORK,
                "name": format!("{}-vnet", vm_name),
                "location": settings.location,
                "properties": {
                    "addressSpace": {
                        "addressPrefixes": ["10.0.0.0/16"]
                    },
                    "subnets": [
                        {
                            "name": "default",
                            "properties": {
                                "addressPrefix": "10.0.0.0/24"
                            }
                        }
                    ]
                }
            },
            {
                "type": "Microsoft.Network/publicIPAddresses",
                "apiVersion": API_VERSION_NETWORK,
                "name": format!("{}-ip", vm_name),
                "location": settings.location,
                "sku": { "name": "Standard" },
                "properties": {
                    "publicIPAllocationMethod": "Static"
                }
            },
            {
                "type": "Microsoft.Network/networkInterfaces",
                "apiVersion": API_VERSION_NETWORK,
                "name": format!("{}-nic", vm_name),
                "location": settings.location,
                "dependsOn": [
                    format!("[resourceId('Microsoft.Network/publicIPAddresses', '{}-ip')]", vm_name),
                    format!("[resourceId('Microsoft.Network/virtualNetworks', '{}-vnet')]", vm_name),
                    format!("[resourceId('Microsoft.Network/networkSecurityGroups', '{}-nsg')]", vm_name)
                ],
                "properties": {
                    "ipConfigurations": [
                        {
                            "name": "ipconfig1",
                            "properties": {
                                "privateIPAllocationMethod": "Dynamic",
                                "publicIPAddress": {
                                    "id": format!("[resourceId('Microsoft.Network/publicIPAddresses', '{}-ip')]", vm_name),
                                    "properties": { "deleteOption": "Delete" }
                                },
                                "subnet": {
                                    "id": format!("[resourceId('Microsoft.Network/virtualNetworks/subnets', '{}-vnet', 'default')]", vm_name)
                                }
                            }
                        }
                    ],
                    "networkSecurityGroup": {
                        "id": format!("[resourceId('Microsoft.Network/networkSecurityGroups', '{}-nsg')]", vm_name)
                    }
                }
            },
            {
                "type": "Microsoft.Compute/virtualMachines",
                "apiVersion": API_VERSION_COMPUTE,
                "name": vm_name,
                "location": settings.location,
                "dependsOn": [
                    format!("[resourceId('Microsoft.Network/networkInterfaces', '{}-nic')]", vm_name)
                ],
                "properties": vm
            }
        ]
    })
}

/// What auto-stop does with an idle VM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VmKind {
    /// On an ephemeral OS disk: it cannot be stopped and kept, so it is
    /// deleted. A VM this app creates on a size with local storage is one.
    Disposable,
    /// On a managed disk, Spot or not: deallocated and started again.
    Kept,
}

/// The kind of VM `vm_name` is, or `None` once Azure no longer has it, as
/// after a Spot eviction.
pub fn vm_kind(client: &Client, vm_name: &str) -> Result<Option<VmKind>> {
    let path = format!(
        "/subscriptions/{}/resourceGroups/{}/providers/Microsoft.Compute/virtualMachines/{vm_name}?api-version={API_VERSION_COMPUTE}",
        client.settings.subscription_id, client.settings.resource_group
    );
    let vm: serde_json::Value = match client.get(&path) {
        Ok(vm) => vm,
        Err(Error::Status(404, _)) => return Ok(None),
        Err(error) => return Err(error),
    };
    Ok(Some(kind_of(&vm)))
}

pub fn kind_of(vm: &serde_json::Value) -> VmKind {
    if vm["properties"]["storageProfile"]["osDisk"]["diffDiskSettings"].is_null() {
        VmKind::Kept
    } else {
        VmKind::Disposable
    }
}

pub fn get_vm_power_state(client: &Client, vm_name: &str) -> Result<PowerState> {
    let path = format!(
        "/subscriptions/{}/resourceGroups/{}/providers/Microsoft.Compute/virtualMachines/{}/instanceView?api-version={}",
        client.settings.subscription_id,
        client.settings.resource_group,
        vm_name,
        API_VERSION_COMPUTE
    );
    let view: InstanceView = client.get(&path)?;
    for item in view.statuses {
        match item.code.as_str() {
            "PowerState/running" => return Ok(PowerState::Running),
            "PowerState/stopped" => return Ok(PowerState::Stopped),
            "PowerState/deallocated" => return Ok(PowerState::Deallocated),
            "PowerState/starting" => return Ok(PowerState::Starting),
            _ => {}
        }
    }
    Ok(PowerState::Unknown)
}

pub fn get_vm_public_ip(client: &Client, vm_name: &str) -> Result<Option<String>> {
    let ip_name = format!("{}-ip", vm_name);
    let path = format!(
        "/subscriptions/{}/resourceGroups/{}/providers/Microsoft.Network/publicIPAddresses/{}?api-version={}",
        client.settings.subscription_id,
        client.settings.resource_group,
        ip_name,
        API_VERSION_NETWORK
    );
    let resp: PublicIpResponse = client.get(&path)?;
    Ok(resp.properties.and_then(|p| p.ip_address))
}

pub fn start_vm(client: &Client, vm_name: &str) -> Result<()> {
    let path = format!(
        "/subscriptions/{}/resourceGroups/{}/providers/Microsoft.Compute/virtualMachines/{}/start?api-version={}",
        client.settings.subscription_id,
        client.settings.resource_group,
        vm_name,
        API_VERSION_COMPUTE
    );
    let _: serde_json::Value = client.post_empty(&path)?;
    Ok(())
}

pub fn deallocate_vm(client: &Client, vm_name: &str) -> Result<()> {
    let path = format!(
        "/subscriptions/{}/resourceGroups/{}/providers/Microsoft.Compute/virtualMachines/{}/deallocate?api-version={}",
        client.settings.subscription_id,
        client.settings.resource_group,
        vm_name,
        API_VERSION_COMPUTE
    );
    let _: serde_json::Value = client.post_empty(&path)?;
    Ok(())
}

/// Delete VM `vm_name` and everything its deployment made, returning once
/// Azure no longer has any of it. `leftovers` runs when the VM itself is
/// gone. A VM deployed with delete options takes its disk, NIC, and IP with
/// it; an older one leaves them, so each is deleted if it is still there.
pub fn delete_vm(
    client: &Client,
    vm_name: &str,
    cancelled: &dyn Fn() -> bool,
    leftovers: impl FnOnce(),
) -> Result<()> {
    let settings = client.settings;
    let vm = format!(
        "/subscriptions/{}/resourceGroups/{}/providers/Microsoft.Compute/virtualMachines/{vm_name}?api-version={API_VERSION_COMPUTE}",
        settings.subscription_id, settings.resource_group
    );
    delete_and_wait(client, &vm, cancelled)?;
    leftovers();
    for path in leftover_paths(settings, vm_name) {
        delete_and_wait(client, &path, cancelled)?;
    }
    Ok(())
}

/// What a VM's deployment leaves once the VM is gone, in an order Azure
/// accepts: the NIC holds the IP and NSG, and the VNet holds the NIC. The
/// shutdown schedule is what the portal's auto-shutdown adds.
pub fn leftover_paths(settings: &Settings, vm_name: &str) -> Vec<String> {
    let group = format!(
        "/subscriptions/{}/resourceGroups/{}/providers",
        settings.subscription_id, settings.resource_group
    );
    vec![
        format!(
            "{group}/Microsoft.DevTestLab/schedules/shutdown-computevm-{vm_name}?api-version=2018-09-15"
        ),
        format!(
            "{group}/Microsoft.Network/networkInterfaces/{vm_name}-nic?api-version={API_VERSION_NETWORK}"
        ),
        format!("{group}/Microsoft.Compute/disks/{vm_name}-osdisk?api-version={API_VERSION_DISKS}"),
        format!(
            "{group}/Microsoft.Network/publicIPAddresses/{vm_name}-ip?api-version={API_VERSION_NETWORK}"
        ),
        format!(
            "{group}/Microsoft.Network/networkSecurityGroups/{vm_name}-nsg?api-version={API_VERSION_NETWORK}"
        ),
        format!(
            "{group}/Microsoft.Network/virtualNetworks/{vm_name}-vnet?api-version={API_VERSION_NETWORK}"
        ),
    ]
}

/// Delete the resource at `path`, then wait until reading it finds nothing.
/// Azure deletes in the background, and a resource still holding another
/// keeps that one from being deleted.
fn delete_and_wait(client: &Client, path: &str, cancelled: &dyn Fn() -> bool) -> Result<()> {
    match client.delete::<serde_json::Value>(path) {
        Ok(_) | Err(Error::Status(404, _)) => {}
        Err(error) => return Err(error),
    }
    let deadline = Instant::now() + TIMEOUT;
    loop {
        match client.get::<serde_json::Value>(path) {
            Err(Error::Status(404, _)) => return Ok(()),
            Err(error) => return Err(error),
            Ok(_) => {}
        }
        if Instant::now() >= deadline {
            return Err(Error::DeleteTimeout);
        }
        pause(cancelled)?;
    }
}

fn pause(cancelled: &dyn Fn() -> bool) -> Result<()> {
    let wake = Instant::now() + POLL_INTERVAL;
    while Instant::now() < wake {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Ok(())
}

/// The size of each VM in the resource group, by lowercase name.
pub fn vm_sizes(client: &Client) -> Result<std::collections::HashMap<String, VmModel>> {
    let path = format!(
        "/subscriptions/{}/resourceGroups/{}/providers/Microsoft.Compute/virtualMachines?api-version={}",
        client.settings.subscription_id, client.settings.resource_group, API_VERSION_COMPUTE
    );
    let list: VmList = client.get(&path)?;
    Ok(list
        .value
        .into_iter()
        .filter_map(|vm| {
            let properties = vm.properties?;
            let model = VmModel {
                size: properties.hardware?.size?,
                spot: properties.priority.as_deref() == Some("Spot"),
            };
            Some((vm.name.to_ascii_lowercase(), model))
        })
        .collect())
}

pub fn wait_running_ip(
    client: &Client,
    vm_name: &str,
    cancelled: &dyn Fn() -> bool,
) -> Result<String> {
    let deadline = Instant::now() + TIMEOUT;
    while Instant::now() < deadline {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        if let Ok(PowerState::Running) = get_vm_power_state(client, vm_name)
            && let Ok(Some(ip)) = get_vm_public_ip(client, vm_name)
            && !ip.is_empty()
        {
            return Ok(ip);
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    Err(Error::StartTimeout)
}
