use super::*;

#[test]
fn local_disk_sizes_place_the_os_disk_where_azure_keeps_it() {
    assert_eq!(ephemeral_placement("Standard_D2plds_v6"), Some("NvmeDisk"));
    assert_eq!(ephemeral_placement("Standard_E4pds_v6"), Some("NvmeDisk"));
    assert_eq!(
        ephemeral_placement("Standard_D2pds_v5"),
        Some("ResourceDisk")
    );
    assert_eq!(
        ephemeral_placement("Standard_D4ds_v4"),
        Some("ResourceDisk")
    );
    // No local storage, so no ephemeral disk.
    for size in ["Standard_B2pts_v2", "Standard_D2ps_v6", "Standard_D4s_v5"] {
        assert_eq!(ephemeral_placement(size), None, "{size}");
    }
}

#[test]
fn every_listed_size_can_hold_an_ephemeral_disk() {
    for size in VM_CHOICES {
        assert_eq!(ephemeral_placement(size), Some("NvmeDisk"), "{size}");
    }
}

#[test]
fn only_the_linux_spot_price_is_kept_as_spot() {
    let items = vec![
        item(
            "Standard_D2plds_v6",
            "D2plds v6",
            "Virtual Machines Dpldsv6 Series",
            0.0882,
        ),
        item(
            "Standard_D2plds_v6",
            "D2plds v6 Spot",
            "Virtual Machines Dpldsv6 Series",
            0.0171,
        ),
        item(
            "Standard_D2plds_v6",
            "D2plds v6 Low Priority",
            "Virtual Machines Dpldsv6 Series",
            0.0150,
        ),
        item(
            "Standard_D2plds_v6",
            "D2plds v6 Spot",
            "Virtual Machines Dpldsv6 Series Windows",
            0.0500,
        ),
    ];
    let spot = spot_prices(items.clone());
    assert_eq!(spot.len(), 1);
    assert_eq!(spot["Standard_D2plds_v6"], Price(0.0171));
    assert_eq!(linux_prices(items)["Standard_D2plds_v6"], Price(0.0882));
}
