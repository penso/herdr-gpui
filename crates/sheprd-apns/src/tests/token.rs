use super::*;

#[test]
fn tokens_are_bounded_lowercase_hex() {
    let token = DeviceToken::parse(&"AB".repeat(32)).unwrap();
    assert_eq!(token.as_str(), "ab".repeat(32));
    for bad in [
        "",
        "abc",
        &"ab".repeat(8 - 1),
        &"ab".repeat(257),
        &"zz".repeat(32),
        &format!("{}/../x", "ab".repeat(32)),
    ] {
        assert!(
            matches!(DeviceToken::parse(bad), Err(Error::DeviceToken)),
            "{bad}"
        );
    }
}
