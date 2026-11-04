use super::*;

#[test]
fn disk_cleanup_uses_the_disk_api_instead_of_the_vm_api_version() {
    let config = AzureConfig {
        subscription_id: Some("11111111-1111-1111-1111-111111111111".into()),
        resource_group: Some("saved-rg".into()),
        ..Default::default()
    };
    let settings = Settings::resolve(&config, |_| None).unwrap().unwrap();
    let paths = api::leftover_paths(&settings, "herdr-box");
    // delete_and_wait uses this same URL for DELETE and subsequent GETs.
    assert!(paths.iter().any(|path| path ==
        "/subscriptions/11111111-1111-1111-1111-111111111111/resourceGroups/saved-rg/providers/Microsoft.Compute/disks/herdr-box-osdisk?api-version=2024-03-02"
    ));
}
