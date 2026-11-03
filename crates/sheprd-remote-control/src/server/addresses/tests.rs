#![allow(clippy::unwrap_used)]
use super::*;

mod ranking;

#[test]
fn metadata_joins_adapters_without_adding_uncertified_origins() {
    let origins = [
        "https://100.84.59.116:7443",
        "https://[fd7a:115c:a1e0::1]:7443",
        "https://10.16.4.184:7443",
        "https://192.168.139.3:7443",
        "https://localhost:7443",
    ]
    .map(String::from);
    let interfaces = [
        ("utun12", "100.84.59.116".parse().unwrap()),
        ("utun12", "fd7a:115c:a1e0::1".parse().unwrap()),
        ("en0", "10.16.4.184".parse().unwrap()),
        ("bridge100", "192.168.139.3".parse().unwrap()),
        ("en0", "10.16.4.184".parse().unwrap()),
        ("en2", "192.168.2.2".parse().unwrap()),
    ];
    let metadata = describe(&origins, &interfaces);
    assert_eq!(metadata.len(), origins.len());
    assert_eq!(metadata[&origins[0]].kind, AddressKind::Tailscale);
    assert_eq!(metadata[&origins[1]].kind, AddressKind::Tailscale);
    assert_eq!(metadata[&origins[0]].interfaces, ["utun12"]);
    assert_eq!(metadata[&origins[2]].interfaces, ["en0"]);
    assert_eq!(metadata[&origins[3]].kind, AddressKind::Bridge);
    assert!(metadata[&origins[4]].interfaces.is_empty());
    assert_eq!(metadata[&origins[4]].kind, AddressKind::Loopback);
}

#[test]
fn cgnat_alone_is_not_proof_of_tailscale_and_missing_adapters_are_honest() {
    let origins = ["https://100.84.59.116:7443".to_owned()];
    let interfaces = [("en0", "100.84.59.116".parse().unwrap())];
    assert_eq!(
        describe(&origins, &interfaces)[&origins[0]].kind,
        AddressKind::Other
    );
    assert!(describe(&origins, &[])[&origins[0]].interfaces.is_empty());
    let tailscale = [("tailscale0", "100.84.59.116".parse().unwrap())];
    assert_eq!(
        describe(&origins, &tailscale)[&origins[0]].kind,
        AddressKind::Tailscale
    );
}
