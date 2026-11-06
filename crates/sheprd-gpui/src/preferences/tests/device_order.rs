//! The device order is remembered with the rest of the chrome.
use super::*;

#[core::prelude::v1::test]
fn the_device_order_round_trips_and_damage_loses_only_itself() {
    let directory = TestDirectory::new();
    let path = directory.0.join("preferences.json");
    let chrome = Chrome {
        sidebar_width: Some(250.),
        device_order: vec!["wsl:Ubuntu".into(), "ssh:a".into()],
        ..Chrome::default()
    };
    write_chrome(&path, chrome.clone()).unwrap();
    assert_eq!(read_chrome(&path).unwrap(), chrome);
    // Files from earlier builds have no order.
    fs::write(&path, r#"{"sidebar_width_px": 250}"#).unwrap();
    assert!(read_chrome(&path).unwrap().device_order.is_empty());
    for damaged in [r#""ssh:a""#, "7", "null", r#"{"ssh:a": 1}"#] {
        fs::write(
            &path,
            format!(r#"{{"sidebar_width_px": 250, "device_order": {damaged}}}"#),
        )
        .unwrap();
        let read = read_chrome(&path).unwrap();
        assert!(read.device_order.is_empty(), "{damaged}");
        assert_eq!(read.sidebar_width, Some(250.), "{damaged}");
    }
    // Entries that are not IDs are dropped; the rest keep their order.
    let long = "x".repeat(MAX_DEVICE_ID + 1);
    fs::write(
        &path,
        format!(r#"{{"device_order": ["ssh:b", 3, "", "{long}", "ssh:a"]}}"#),
    )
    .unwrap();
    assert_eq!(read_chrome(&path).unwrap().device_order, ["ssh:b", "ssh:a"]);
}
