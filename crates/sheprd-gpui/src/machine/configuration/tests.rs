#![allow(clippy::expect_used)]
use super::*;

#[test]
fn a_machine_keeps_its_address_between_launches() {
    // A new address at each launch makes every saved state refuse to restore.
    assert_eq!(mac_address(b"identity"), mac_address(b"identity"));
}

#[test]
fn machines_get_different_addresses() {
    assert_ne!(mac_address(b"one"), mac_address(b"two"));
}

#[test]
fn an_address_is_locally_administered_and_unicast() {
    for identity in [&b"one"[..], b"two", b"three", b""] {
        let address = mac_address(identity);
        assert_eq!(address.split(':').count(), 6, "{address}");
        let first = u8::from_str_radix(&address[..2], 16).expect("hex");
        assert_eq!(first & 0b11, 0b10, "{address}");
    }
}
