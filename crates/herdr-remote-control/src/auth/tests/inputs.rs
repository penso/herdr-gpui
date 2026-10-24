use super::*;

#[test]
fn device_names_are_trimmed_and_bounded() {
    assert_eq!(device_name("  iPhone 15 ").unwrap(), "iPhone 15");
    assert_eq!(device_name(&"é".repeat(64)).unwrap(), "é".repeat(64));
    for bad in [
        String::new(),
        "   ".to_owned(),
        "a".repeat(65),
        "line\nbreak".to_owned(),
        "bell\u{7}".to_owned(),
        "del\u{7f}".to_owned(),
        "c1\u{85}x".to_owned(),
    ] {
        assert!(
            matches!(device_name(&bad), Err(Error::InvalidName)),
            "{bad:?}"
        );
    }
}

#[test]
fn a_webcrypto_raw_key_parses_and_round_trips_as_base64url() {
    let encoded = public_key(&signing_key(3));
    let key = PublicKey::parse(&encoded).unwrap();
    assert_eq!(key.as_bytes().len(), 65);
    assert_eq!(String::from(key.clone()), encoded);
    let json = serde_json::to_string(&key).unwrap();
    assert_eq!(json, format!("\"{encoded}\""));
    assert_eq!(serde_json::from_str::<PublicKey>(&json).unwrap(), key);
}

#[test]
fn malformed_keys_are_rejected() {
    let key = signing_key(3);
    let compressed = encode(key.verifying_key().to_sec1_point(true).as_bytes());
    let mut off_curve = [0u8; 65];
    off_curve[0] = 4;
    let mut wrong_tag = key.verifying_key().to_sec1_point(false).as_bytes().to_vec();
    wrong_tag[0] = 6;
    let good = public_key(&key);
    for bad in [
        String::new(),
        compressed,
        encode(&off_curve),
        encode(&wrong_tag),
        "!".repeat(87),
        format!("{good}="),
        good.replace('-', "+").replace('_', "/") + "+",
        "A".repeat(100_000),
    ] {
        assert!(
            matches!(PublicKey::parse(&bad), Err(Error::InvalidKey)),
            "{bad:.20}"
        );
    }
}

#[test]
fn fixed_decoding_refuses_any_other_length() {
    assert!(decode_fixed::<4>(&encode(&[1, 2, 3, 4])).is_ok());
    assert!(decode_fixed::<4>(&encode(&[1, 2, 3])).is_err());
    assert!(decode_fixed::<4>(&encode(&[1, 2, 3, 4, 5])).is_err());
    assert!(decode_fixed::<4>("AQIDBA==").is_err());
}

#[test]
fn constant_time_comparison_matches_equality() {
    assert!(constant_time_eq(b"secret", b"secret"));
    assert!(!constant_time_eq(b"secret", b"secreT"));
    assert!(!constant_time_eq(b"secret", b"secrets"));
    assert!(constant_time_eq(b"", b""));
}

#[test]
fn errors_map_to_control_codes() {
    use control::ErrorCode;
    assert_eq!(
        ErrorCode::from(&Error::UnknownPairing),
        ErrorCode::UnknownPairing
    );
    assert_eq!(
        ErrorCode::from(&Error::UnknownDevice),
        ErrorCode::UnknownDevice
    );
    assert_eq!(ErrorCode::from(&Error::NotClaimed), ErrorCode::NotClaimed);
    assert_eq!(ErrorCode::from(&Error::InvalidToken), ErrorCode::Other);
}

#[test]
fn an_auth_error_is_a_crate_error() {
    let error = crate::Error::from(Error::UnknownDevice);
    assert!(matches!(error, crate::Error::Auth(Error::UnknownDevice)));
    assert_eq!(error.to_string(), "no such device");
}
