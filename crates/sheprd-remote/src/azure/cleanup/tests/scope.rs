use super::*;

fn device(account: String, id: &str) -> SavedDevice {
    SavedDevice {
        provider: CloudProvider::Azure,
        id: id.into(),
        label: id.into(),
        account,
        machine: id.into(),
        session: "default".into(),
        enabled: true,
    }
}

#[test]
fn cleanup_checks_saved_groups_even_when_auto_stop_is_disabled() {
    let mut current = settings();
    current.resource_group = "new-rg".into();
    current.sandbox_group = "new-boxes".into();
    current.sandbox_location = "westus2".into();
    current.auto_stop = None;
    let devices = vec![
        device(format!("{MINE}/old-rg"), "first-vm"),
        device(format!("{MINE}/old-rg"), "second-vm"),
        device(format!("{MINE}/old-rg/old-boxes/eastus"), "sandbox.box"),
        device(current.vm_account(), "current-vm"),
    ];
    let mut checked = BTreeMap::new();
    check_accounts(&current, devices, |scoped, devices| {
        assert_eq!(scoped.auto_stop, None);
        for device in devices {
            checked.insert(
                device.id.clone(),
                (
                    scoped.resource_group.clone(),
                    scoped.sandbox_group.clone(),
                    scoped.sandbox_location.clone(),
                ),
            );
        }
        if scoped.resource_group == "old-rg" && scoped.sandbox_group == "new-boxes" {
            assert_eq!(
                devices
                    .iter()
                    .map(|device| device.id.as_str())
                    .collect::<Vec<_>>(),
                ["first-vm", "second-vm"]
            );
        }
    });
    let inherited = ("old-rg".into(), "new-boxes".into(), "westus2".into());
    assert_eq!(checked.get("first-vm"), Some(&inherited));
    assert_eq!(checked.get("second-vm"), Some(&inherited));
    assert_eq!(
        checked.get("sandbox.box"),
        Some(&("old-rg".into(), "old-boxes".into(), "eastus".into()))
    );
    assert_eq!(
        checked.get("current-vm"),
        Some(&("new-rg".into(), "new-boxes".into(), "westus2".into()))
    );
}

#[test]
fn cleanup_skips_invalid_accounts_and_keeps_checking_valid_devices() {
    let current = settings();
    let devices = vec![
        device(format!("{MINE}/"), "damaged"),
        device(format!("{OTHER}/rg"), "other-subscription"),
        device(MINE.into(), "legacy"),
    ];
    let mut checked = Vec::new();
    check_accounts(&current, devices, |scoped, devices| {
        assert_eq!(scoped, &current);
        checked.extend(devices.iter().map(|device| device.id.clone()));
    });
    assert_eq!(checked, ["legacy"]);
}
