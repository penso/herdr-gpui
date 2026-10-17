use super::*;

const SUBSCRIPTION: &str = "11111111-1111-1111-1111-111111111111";

fn settings(resource_group: &str, sandbox_group: &str, location: &str) -> Settings {
    let config = AzureConfig {
        subscription_id: Some(SUBSCRIPTION.into()),
        resource_group: Some(resource_group.into()),
        sandbox_group: Some(sandbox_group.into()),
        sandbox_location: Some(location.into()),
        ..Default::default()
    };
    Settings::resolve(&config, |_| None).unwrap().unwrap()
}

#[test]
fn a_saved_vm_keeps_its_group_after_the_setting_changes() {
    let saved = settings("first-rg", "boxes", "centralus").vm_account();
    let now = settings("second-rg", "boxes", "centralus");

    let scoped = now.for_device(&saved).unwrap();

    assert_eq!(scoped.resource_group, "first-rg");
    assert_eq!(now.group_of(&saved), "first-rg");
}

#[test]
fn a_saved_sandbox_keeps_its_group_and_region() {
    let saved = settings("first-rg", "first-boxes", "eastus").sandbox_account();
    let now = settings("second-rg", "second-boxes", "westus2");

    let scoped = now.for_device(&saved).unwrap();

    assert_eq!(scoped.resource_group, "first-rg");
    assert_eq!(scoped.sandbox_group, "first-boxes");
    assert_eq!(scoped.sandbox_location, "eastus");
}

#[test]
fn an_older_device_follows_the_configured_groups() {
    let now = settings("current-rg", "boxes", "centralus");

    let scoped = now.for_device(SUBSCRIPTION).unwrap();

    assert_eq!(scoped, now);
    assert_eq!(now.group_of(SUBSCRIPTION), "current-rg");
}

#[test]
fn another_subscriptions_device_is_refused() {
    let now = settings("rg", "boxes", "centralus");
    let other = "22222222-2222-2222-2222-222222222222/rg";

    assert!(!now.owns(other));
    assert!(matches!(now.for_device(other), Err(Error::Account)));
}

#[test]
fn a_damaged_account_is_refused() {
    let now = settings("rg", "boxes", "centralus");
    for damaged in [
        format!("{SUBSCRIPTION}/"),
        format!("{SUBSCRIPTION}/rg/boxes"),
        format!("{SUBSCRIPTION}/rg//centralus"),
        format!("{SUBSCRIPTION}/rg/boxes/centralus/extra"),
        format!("{SUBSCRIPTION}/rg/box?es/centralus"),
    ] {
        assert!(
            matches!(now.for_device(&damaged), Err(Error::Field(_))),
            "{damaged}"
        );
    }
}
