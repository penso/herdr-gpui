use super::*;

#[test]
fn a_carrier_address_cannot_displace_the_lan_default() {
    let mut origins = ["https://100.84.59.116:7443", "https://192.168.1.20:7443"].map(String::from);
    let interfaces = [
        ("wwan0", "100.84.59.116".parse().unwrap()),
        ("en0", "192.168.1.20".parse().unwrap()),
    ];
    let details = describe(&origins, &interfaces);
    rank(&mut origins, &details);
    assert_eq!(origins[0], "https://192.168.1.20:7443");
    assert_eq!(details[&origins[1]].kind, AddressKind::Other);
}

#[test]
fn confirmed_tailscale_ipv4_leads_without_promoting_unrelated_cgnat() {
    let mut origins = [
        "https://100.84.59.116:7443",
        "https://[fd7a:115c:a1e0::3]:7443",
        "https://192.168.1.20:7443",
        "https://100.64.0.3:7443",
    ]
    .map(String::from);
    let interfaces = [
        ("wwan0", "100.84.59.116".parse().unwrap()),
        ("en0", "192.168.1.20".parse().unwrap()),
        ("utun12", "100.64.0.3".parse().unwrap()),
        ("utun12", "fd7a:115c:a1e0::3".parse().unwrap()),
    ];
    let details = describe(&origins, &interfaces);
    rank(&mut origins, &details);
    assert_eq!(
        origins,
        [
            "https://100.64.0.3:7443",
            "https://[fd7a:115c:a1e0::3]:7443",
            "https://192.168.1.20:7443",
            "https://100.84.59.116:7443",
        ]
    );
    assert_eq!(details[&origins[0]].kind, AddressKind::Tailscale);
    assert_eq!(details[&origins[3]].kind, AddressKind::Other);
}

#[test]
fn tailscale_dns_is_labeled_and_ranked_consistently_for_custom_certificates() {
    let mut origins = [
        "https://192.168.1.20:7443",
        "https://studio.example.ts.net:7443",
    ]
    .map(String::from);
    let details = describe(&origins, &[]);
    rank(&mut origins, &details);
    assert_eq!(origins[0], "https://studio.example.ts.net:7443");
    assert_eq!(details[&origins[0]].kind, AddressKind::Tailscale);
}
