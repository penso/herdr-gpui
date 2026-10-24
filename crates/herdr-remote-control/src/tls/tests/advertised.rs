//! The names a server advertises: what its bind address answers at, and
//! what a user's own certificate lists.

use super::*;

#[test]
fn a_single_bind_address_advertises_only_itself() {
    let names = lan_names();
    assert_eq!(names.clone().bound_to(ip("::")), names);
    assert_eq!(names.clone().bound_to(ip("0.0.0.0")), names);
    let tailscale = names.clone().bound_to(ip("100.101.102.103"));
    assert_eq!(
        tailscale.origins(7443),
        ["https://100.101.102.103:7443"],
        "{tailscale:?}"
    );
    let loopback = names.clone().bound_to(ip("127.0.0.1"));
    assert_eq!(
        loopback.origins(7443),
        ["https://127.0.0.1:7443", "https://localhost:7443"]
    );
    // An address this machine does not list advertises nothing.
    assert!(names.bound_to(ip("10.9.9.9")).origins(7443).is_empty());
}

#[test]
fn a_user_certificate_advertises_the_names_it_lists() {
    let mut params = rcgen::CertificateParams::new(vec![
        "remote.example.com".to_owned(),
        "*.example.com".to_owned(),
    ])
    .unwrap();
    params
        .subject_alt_names
        .push(rcgen::SanType::IpAddress(ip("203.0.113.7")));
    let key = rcgen::KeyPair::generate().unwrap();
    let cert = params.self_signed(&key).unwrap();
    let names = Names::from_certificate(cert.der());
    assert_eq!(names.dns, ["remote.example.com"]);
    assert_eq!(names.ips, [ip("203.0.113.7")]);
    assert_eq!(
        names.clone().bound_to(ip("::")).origins(443),
        ["https://203.0.113.7", "https://remote.example.com"]
    );
    // Bound to one address, the certificate's own domain still reaches it.
    assert_eq!(
        names.bound_to(ip("192.168.1.20")).origins(443),
        ["https://remote.example.com"]
    );
    assert_eq!(
        Names::from_certificate(b"not a certificate"),
        Names::default()
    );
}

#[test]
fn port_443_origins_read_as_browsers_send_them() {
    let loopback = lan_names().bound_to(ip("127.0.0.1"));
    assert_eq!(
        loopback.origins(443),
        ["https://127.0.0.1", "https://localhost"]
    );
}
