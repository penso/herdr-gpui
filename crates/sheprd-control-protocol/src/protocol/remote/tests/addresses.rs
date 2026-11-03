use super::*;

#[test]
fn pairing_address_is_optional_and_round_trips() {
    let old = RemoteOp::Pair { address: None };
    assert_eq!(serde_json::to_string(&old).unwrap(), r#"{"op":"pair"}"#);
    let selected = RemoteOp::Pair {
        address: Some("https://100.64.0.3:7443".into()),
    };
    let wire = serde_json::to_string(&selected).unwrap();
    assert_eq!(serde_json::from_str::<RemoteOp>(&wire).unwrap(), selected);
    assert_eq!(selected.wait_seconds(), 0);
}
