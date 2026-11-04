use super::*;

fn settings(vm_size: &str) -> Settings {
    let config = AzureConfig {
        subscription_id: Some("11111111-1111-1111-1111-111111111111".into()),
        vm_size: Some(vm_size.into()),
        ..Default::default()
    };
    Settings::resolve(&config, |_| None).unwrap().unwrap()
}

fn template(size: &str, priority: api::Priority) -> serde_json::Value {
    api::vm_template(&settings(size), "herdr-box", "ssh-ed25519 KEY", priority)
}

fn resource<'a>(template: &'a serde_json::Value, kind: &str) -> &'a serde_json::Value {
    template["resources"]
        .as_array()
        .unwrap()
        .iter()
        .find(|resource| resource["type"] == kind)
        .unwrap()
}

#[test]
fn a_deleted_vm_takes_its_disk_nic_and_public_ip_with_it() {
    let template = template(DEFAULT_VM_SIZE, api::Priority::Spot);
    let vm = &resource(&template, "Microsoft.Compute/virtualMachines")["properties"];
    let disk = &vm["storageProfile"]["osDisk"];
    assert_eq!(disk["deleteOption"], "Delete");
    for nic in vm["networkProfile"]["networkInterfaces"]
        .as_array()
        .unwrap()
    {
        assert_eq!(nic["properties"]["deleteOption"], "Delete");
    }
    let nic = &resource(&template, "Microsoft.Network/networkInterfaces")["properties"];
    for config in nic["ipConfigurations"].as_array().unwrap() {
        assert_eq!(
            config["properties"]["publicIPAddress"]["properties"]["deleteOption"],
            "Delete"
        );
    }
}

#[test]
fn the_default_size_is_arm_and_gets_the_arm_image() {
    assert!(sizes::is_arm(DEFAULT_VM_SIZE));
    let template = template(DEFAULT_VM_SIZE, api::Priority::Spot);
    let vm = &resource(&template, "Microsoft.Compute/virtualMachines")["properties"];
    assert_eq!(vm["hardwareProfile"]["vmSize"], DEFAULT_VM_SIZE);
    assert_eq!(
        vm["storageProfile"]["imageReference"]["sku"],
        "22_04-lts-arm64"
    );
}

#[test]
fn a_new_vm_is_spot_with_its_os_disk_on_local_nvme() {
    let template = template(DEFAULT_VM_SIZE, api::Priority::Spot);
    let vm = &resource(&template, "Microsoft.Compute/virtualMachines")["properties"];
    assert_eq!(vm["priority"], "Spot");
    // An ephemeral disk cannot be deallocated, so eviction deletes.
    assert_eq!(vm["evictionPolicy"], "Delete");
    assert_eq!(vm["billingProfile"]["maxPrice"], -1);
    let disk = &vm["storageProfile"]["osDisk"];
    assert_eq!(disk["diffDiskSettings"]["option"], "Local");
    assert_eq!(disk["diffDiskSettings"]["placement"], "NvmeDisk");
    assert_eq!(disk["caching"], "ReadOnly");
    // No managed disk, so nothing is billed for it.
    assert!(disk.get("managedDisk").is_none());
}

#[test]
fn a_size_without_local_storage_gets_a_managed_disk_it_can_be_deallocated_with() {
    let template = template("Standard_B2pts_v2", api::Priority::Spot);
    let vm = &resource(&template, "Microsoft.Compute/virtualMachines")["properties"];
    let disk = &vm["storageProfile"]["osDisk"];
    assert!(disk.get("diffDiskSettings").is_none());
    assert_eq!(disk["managedDisk"]["storageAccountType"], "StandardSSD_LRS");
    assert_eq!(disk["deleteOption"], "Delete");
    // Evicted, it is kept deallocated rather than deleted.
    assert_eq!(vm["evictionPolicy"], "Deallocate");
    assert_eq!(
        api::kind_of(resource(&template, "Microsoft.Compute/virtualMachines")),
        api::VmKind::Kept
    );
}

#[test]
fn a_regular_vm_has_no_spot_settings_and_keeps_its_disk_choice() {
    let template = template(DEFAULT_VM_SIZE, api::Priority::Regular);
    let vm = &resource(&template, "Microsoft.Compute/virtualMachines")["properties"];
    for key in ["priority", "evictionPolicy", "billingProfile"] {
        assert!(vm.get(key).is_none(), "{key}");
    }
    assert_eq!(
        vm["storageProfile"]["osDisk"]["diffDiskSettings"]["placement"],
        "NvmeDisk"
    );
}

#[test]
fn auto_stop_destroys_what_the_template_makes_and_deallocates_older_vms() {
    let template = template(DEFAULT_VM_SIZE, api::Priority::Spot);
    let vm = resource(&template, "Microsoft.Compute/virtualMachines");
    assert_eq!(api::kind_of(vm), api::VmKind::Disposable);
    let older = serde_json::json!({"properties": {"storageProfile": {"osDisk": {
        "managedDisk": {"storageAccountType": "StandardSSD_LRS"}
    }}}});
    assert_eq!(api::kind_of(&older), api::VmKind::Kept);
}

#[test]
fn azure_errors_carry_their_inner_reasons_and_codes() {
    let body = br#"{"error":{"code":"InvalidTemplateDeployment","message":"The template deployment is not valid. See inner errors for details.","details":[{"code":"SkuNotAvailable","message":"The requested VM size Standard_D2plds_v6 is currently not available in location 'centralus'."}]}}"#;
    let text = http::diagnostic(body);
    assert!(
        text.starts_with("The template deployment is not valid."),
        "{text}"
    );
    assert!(
        text.contains("Standard_D2plds_v6 is currently not available"),
        "{text}"
    );
    assert_eq!(
        http::codes(body),
        ["InvalidTemplateDeployment", "SkuNotAvailable"]
    );
    assert_eq!(http::diagnostic(br#"{"message":"plain"}"#), "plain");
    assert_eq!(http::diagnostic(b"not json"), "not json");
}
