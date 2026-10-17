#![allow(clippy::unwrap_used)]
//! Unit tests for Azure cloud device provider.

use super::*;
use crate::config::AzureConfig;
use settings::{DEFAULT_ADMIN_USERNAME, DEFAULT_LOCATION, DEFAULT_RESOURCE_GROUP, DEFAULT_VM_SIZE};

#[test]
fn settings_resolution_with_explicit_values() {
    let config = AzureConfig {
        subscription_id: Some("11111111-1111-1111-1111-111111111111".into()),
        resource_group: Some("my-rg".into()),
        location: Some("eastus2".into()),
        vm_size: Some("Standard_B2pts_v2".into()),
        tenant_id: Some("22222222-2222-2222-2222-222222222222".into()),
        client_id: Some("33333333-3333-3333-3333-333333333333".into()),
        ..Default::default()
    };

    let settings = Settings::resolve(&config, |_| None).unwrap().unwrap();
    assert_eq!(
        settings.subscription_id,
        "11111111-1111-1111-1111-111111111111"
    );
    assert_eq!(settings.resource_group, "my-rg");
    assert_eq!(settings.location, "eastus2");
    assert_eq!(settings.vm_size, "Standard_B2pts_v2");
    assert_eq!(
        settings.vm_account(),
        "11111111-1111-1111-1111-111111111111/my-rg"
    );
}

#[test]
fn settings_resolution_defaults() {
    let config = AzureConfig {
        subscription_id: Some("11111111-1111-1111-1111-111111111111".into()),
        ..Default::default()
    };

    let settings = Settings::resolve(&config, |_| None).unwrap().unwrap();
    assert_eq!(settings.resource_group, DEFAULT_RESOURCE_GROUP);
    assert_eq!(settings.location, DEFAULT_LOCATION);
    assert_eq!(settings.vm_size, DEFAULT_VM_SIZE);
    assert_eq!(settings.admin_username, DEFAULT_ADMIN_USERNAME);
}

#[test]
fn settings_invalid_subscription_guid() {
    let config = AzureConfig {
        subscription_id: Some("not-a-valid-guid".into()),
        ..Default::default()
    };

    let error = Settings::resolve(&config, |_| None).unwrap_err();
    assert!(matches!(error, Error::Field("azure.subscription_id")));
}

#[test]
fn persistent_key_generation() {
    let (path, pubkey) = gateway::persistent_key().unwrap();
    assert!(path.exists());
    assert!(pubkey.starts_with("ssh-ed25519 "));
}

#[test]
fn auto_stop_parsing() {
    use std::time::Duration;
    assert_eq!(settings::parse_auto_stop("").unwrap(), None);
    assert_eq!(settings::parse_auto_stop("never").unwrap(), None);
    assert_eq!(
        settings::parse_auto_stop("30m").unwrap(),
        Some(Duration::from_secs(1800))
    );
    assert_eq!(
        settings::parse_auto_stop("15m").unwrap(),
        Some(Duration::from_secs(900))
    );
    assert_eq!(
        settings::parse_auto_stop("1h").unwrap(),
        Some(Duration::from_secs(3600))
    );
    assert_eq!(
        settings::parse_auto_stop("2h").unwrap(),
        Some(Duration::from_secs(7200))
    );
    assert_eq!(
        settings::parse_auto_stop("4h").unwrap(),
        Some(Duration::from_secs(14400))
    );
    assert_eq!(
        settings::parse_auto_stop("120s").unwrap(),
        Some(Duration::from_secs(120))
    );
}

#[test]
fn settings_resolution_with_auto_stop() {
    use std::time::Duration;
    let config = AzureConfig {
        subscription_id: Some("11111111-1111-1111-1111-111111111111".into()),
        auto_stop: Some("1h".into()),
        ..Default::default()
    };

    let settings = Settings::resolve(&config, |_| None).unwrap().unwrap();
    assert_eq!(settings.auto_stop, Some(Duration::from_secs(3600)));

    let config_never = AzureConfig {
        subscription_id: Some("11111111-1111-1111-1111-111111111111".into()),
        auto_stop: Some("never".into()),
        ..Default::default()
    };

    let settings_never = Settings::resolve(&config_never, |_| None).unwrap().unwrap();
    assert_eq!(settings_never.auto_stop, None);
}

mod check;
mod destroy;
mod scope;
mod template;
