use super::*;

#[test]
fn checking_a_cli_account_still_validates_the_other_keys() {
    // No subscription: resolving would ask `az`; checking must not, yet a
    // bad key elsewhere in the table is still refused.
    let config = AzureConfig {
        use_azure_cli: Some(true),
        ..Default::default()
    };
    assert!(Settings::check(&config, |_| None).is_ok());
    let broken = AzureConfig {
        sandbox_cpu: Some("lots".into()),
        ..config
    };
    assert!(matches!(
        Settings::check(&broken, |_| None),
        Err(Error::Field("azure.sandbox_cpu"))
    ));
}
