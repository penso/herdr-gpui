use super::*;
use herdr_remote_control::control::{AddressDetails, AddressKind};

#[test]
fn status_names_the_daemons_interfaces_in_text_and_json() {
    let mut status = status();
    status.address_details.insert(
        status.addresses[1].clone(),
        AddressDetails {
            interfaces: vec!["utun12".into()],
            kind: AddressKind::Tailscale,
        },
    );
    let output = remote::output(&Reply::Status {
        enabled: true,
        status: Some(Box::new(status)),
        error: None,
    })
    .unwrap();
    assert!(
        output
            .text
            .contains("Address: https://100.64.0.3:7443 — Tailscale · utun12")
    );
    assert_eq!(
        output.json["status"]["address_details"]["https://100.64.0.3:7443"]["interfaces"][0],
        "utun12"
    );
}
