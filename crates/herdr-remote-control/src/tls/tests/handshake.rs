//! Which certificate a handshake gets, by the local address it reached and
//! whether the client sent a name.

use super::*;

#[test]
fn a_nameless_handshake_on_an_unlisted_address_gets_the_dns_leaf() {
    let state = tempfile::tempdir().unwrap();
    let authority = Authority::load_or_create(state.path()).unwrap();
    let now = SystemTime::now();
    let names = Names::new([ip("192.168.1.20"), ip("fe80::1")], Some("studio"));
    let resolver = Resolver::issue(&authority, names, now).unwrap();
    let dns = resolver.current();

    // `studio.local` resolves to link-local IPv6 on the phone, which sends
    // no SNI for it; the DNS leaf is the one that names `studio.local`.
    let link_local = resolver.for_handshake(ip("fe80::1"), false);
    assert!(Arc::ptr_eq(&link_local, &dns));
    verify(&authority, &link_local, "studio.local", now).unwrap();

    let lan = resolver.for_handshake(ip("192.168.1.20"), false);
    assert!(!Arc::ptr_eq(&lan, &dns));
    verify(&authority, &lan, "192.168.1.20", now).unwrap();

    assert!(Arc::ptr_eq(
        &resolver.for_handshake(ip("192.168.1.20"), true),
        &dns
    ));
}
