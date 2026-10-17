use super::*;
use crate::cloud::{CloudProvider, SavedDevice};

fn settings() -> Settings {
    let config = AzureConfig {
        subscription_id: Some("11111111-1111-1111-1111-111111111111".into()),
        ..Default::default()
    };
    Settings::resolve(&config, |_| None).unwrap().unwrap()
}

fn device(provider: CloudProvider, account: &str) -> SavedDevice {
    SavedDevice {
        provider,
        id: "herdr-box".into(),
        label: "herdr-box".into(),
        account: account.into(),
        machine: "herdr-box".into(),
        session: "default".into(),
        enabled: true,
    }
}

#[test]
fn leftovers_are_deleted_in_an_order_azure_accepts() {
    let paths = api::leftover_paths(&settings(), "herdr-box");
    let at = |kind: &str| {
        paths
            .iter()
            .position(|path| path.contains(kind))
            .unwrap_or_else(|| panic!("{kind} is deleted"))
    };
    let nic = at("/networkInterfaces/herdr-box-nic?");
    assert!(
        nic < at("/publicIPAddresses/herdr-box-ip?"),
        "the NIC holds the IP"
    );
    assert!(
        nic < at("/networkSecurityGroups/herdr-box-nsg?"),
        "and the NSG"
    );
    assert!(
        nic < at("/virtualNetworks/herdr-box-vnet?"),
        "and its subnet"
    );
    at("/disks/herdr-box-osdisk?");
    for path in &paths {
        assert!(path.starts_with(
            "/subscriptions/11111111-1111-1111-1111-111111111111/resourceGroups/herdr-devices/providers/"
        ));
        assert!(path.contains("?api-version="));
    }
}

#[test]
fn destroying_refuses_a_device_of_another_account_before_any_request() {
    let settings = settings();
    let never = || -> bool { panic!("no request runs") };
    let elsewhere = CloudProvider::ALL
        .iter()
        .filter(|&&provider| provider != CloudProvider::Azure)
        .map(|&provider| device(provider, &settings.vm_account()));
    let others = std::iter::once(device(
        CloudProvider::Azure,
        "22222222-2222-2222-2222-222222222222",
    ))
    .chain(elsewhere);
    for other in others {
        let result = crate::azure::destroy::destroy_device(&settings, &other, &never, |_| {
            panic!("no stage starts")
        });
        assert!(matches!(result, Err(Error::Account)), "{other:?}");
    }
}
