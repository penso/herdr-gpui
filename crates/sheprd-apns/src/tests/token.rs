use super::*;
use secrecy::ExposeSecret as _;

#[test]
fn tokens_are_bounded_lowercase_hex() {
    let token = DeviceToken::parse(&"AB".repeat(32)).unwrap();
    assert_eq!(token.expose_secret(), "ab".repeat(32));
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

#[test]
fn debug_never_prints_the_token() {
    let hex = "ab".repeat(32);
    let token = DeviceToken::parse(&hex).unwrap();
    let printed = format!("{token:?} {:?}", Some(&token));
    assert!(!printed.contains(&hex), "{printed}");
}

#[test]
fn a_token_still_stores_as_its_hex() {
    let hex = "ab".repeat(32);
    let token = DeviceToken::parse(&hex).unwrap();
    assert_eq!(serde_json::to_value(&token).unwrap(), hex);
    let read: DeviceToken = serde_json::from_value(serde_json::json!(hex)).unwrap();
    assert_eq!(read, token);
}
