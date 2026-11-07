use super::*;

#[test]
fn old_offers_work_and_metadata_round_trips_without_changing_addresses() {
    let old = json!({"id":"p1", "url":"https://100.64.0.1:7443/pair#t=TOKEN", "fingerprint":"AB", "expires_in_ms":60000, "addresses":["https://100.64.0.1:7443"]});
    let mut offer: Offer = serde_json::from_value(old.clone()).unwrap();
    assert!(offer.address_details.is_empty());
    assert_eq!(serde_json::to_value(&offer).unwrap(), old);
    offer.address_details.insert(
        offer.addresses[0].clone(),
        AddressDetails {
            interfaces: vec!["utun12".into()],
            kind: AddressKind::Tailscale,
        },
    );
    let wire = serde_json::to_value(&offer).unwrap();
    assert_eq!(wire["addresses"], old["addresses"]);
    assert_eq!(serde_json::from_value::<Offer>(wire).unwrap(), offer);
    assert_eq!(
        serde_json::from_value::<AddressKind>(json!("future_network")).unwrap(),
        AddressKind::Other
    );
}

#[test]
fn old_status_has_no_interface_metadata() {
    let old = json!({"version":"1", "port":7443, "addresses":[], "fingerprint":"AB", "herdr":false, "chats":0,"devices":0,"connected":0});
    let status: Status = serde_json::from_value(old.clone()).unwrap();
    assert!(status.address_details.is_empty());
    assert_eq!(serde_json::to_value(status).unwrap(), old);
}
